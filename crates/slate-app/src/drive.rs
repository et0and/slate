//! Google Drive OAuth, account persistence and file transfer.
//!
//! `google-drive` owns OAuth and folder listing. Transfers use a streaming
//! request so a 10,000-photo export never has to hold a whole PDF in memory.
//!
//! OAuth credentials are read at runtime and are never compiled into the binary.

use std::collections::HashMap;
use std::future::Future;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use google_drive::Client;
use google_drive::about::About;
use google_drive::files::Files;
use google_drive::types::File;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use url::Url;
use uuid::Uuid;

use slate_core::import;
use slate_core::store::data_directory;

const REDIRECT_URI: &str = "http://127.0.0.1:53682/oauth2callback";
const CALLBACK_PATH: &str = "/oauth2callback";
const DRIVE_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
const DRIVE_FILE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const FOLDER_QUERY: &str = "trashed = false";
const DOWNLOAD_BASE: &str = "https://www.googleapis.com/drive/v3/files";
const UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3/files";

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

#[derive(Debug, Deserialize)]
struct OAuthConfigFile {
    client_id: String,
    client_secret: String,
}

#[derive(Debug, Clone)]
pub struct DriveEntry {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub is_folder: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DriveAccount {
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub name: String,
}

impl DriveAccount {
    pub fn label(&self) -> String {
        if !self.email.is_empty() {
            self.email.clone()
        } else if !self.name.is_empty() {
            self.name.clone()
        } else {
            "Google Drive".to_string()
        }
    }
}

#[derive(Debug, Default)]
pub struct DownloadSummary {
    pub paths: Vec<PathBuf>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UploadedFile {
    pub id: String,
    pub name: String,
    pub url: String,
}

pub fn oauth_config_path() -> PathBuf {
    data_directory().join("google-oauth.json")
}

pub fn token_path() -> PathBuf {
    data_directory().join("google-drive.json")
}

/// Resolve an OAuth client from the environment, a local config file, or a
/// value compiled into a release build.
pub fn oauth_config() -> Result<OAuthConfig> {
    if let (Ok(client_id), Ok(client_secret)) = (
        std::env::var("SLATE_GOOGLE_CLIENT_ID"),
        std::env::var("SLATE_GOOGLE_CLIENT_SECRET"),
    ) && !client_id.trim().is_empty()
        && !client_secret.trim().is_empty()
    {
        return Ok(OAuthConfig {
            client_id: client_id.trim().to_string(),
            client_secret: client_secret.trim().to_string(),
            redirect_uri: REDIRECT_URI.to_string(),
        });
    }

    let path = oauth_config_path();
    if path.is_file() {
        let data =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let file: OAuthConfigFile =
            serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))?;
        if !file.client_id.trim().is_empty() && !file.client_secret.trim().is_empty() {
            return Ok(OAuthConfig {
                client_id: file.client_id.trim().to_string(),
                client_secret: file.client_secret.trim().to_string(),
                redirect_uri: REDIRECT_URI.to_string(),
            });
        }
    }

    bail!(
        "Google Drive is not configured. Add a Desktop OAuth client to {}.",
        path.display()
    )
}

pub fn setup_steps() -> Vec<String> {
    vec![
        "Create or select a Google Cloud project.".to_string(),
        "Enable the Google Drive API.".to_string(),
        "Configure the OAuth consent screen as External while testing, and add your Google account as a test user.".to_string(),
        "Add the drive.readonly and drive.file scopes.".to_string(),
        "Create an OAuth client ID of type Desktop app.".to_string(),
        format!(
            "Save the client ID and secret in {} as {{\"client_id\":\"…\",\"client_secret\":\"…\"}}.",
            oauth_config_path().display()
        ),
        "Save the file, then choose Drive → Link Google Drive.".to_string(),
        "The browser returns to http://127.0.0.1:53682/oauth2callback automatically.".to_string(),
    ]
}

pub fn load_account() -> Option<DriveAccount> {
    let path = token_path();
    let data = std::fs::read_to_string(path).ok()?;
    let account: DriveAccount = serde_json::from_str(&data).ok()?;
    (!account.refresh_token.is_empty()).then_some(account)
}

pub fn save_account(account: &DriveAccount) -> Result<()> {
    let path = token_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let data = serde_json::to_vec_pretty(account)?;
    std::fs::write(&path, data).with_context(|| format!("write {}", path.display()))?;
    restrict_permissions(&path)?;
    Ok(())
}

pub fn clear_account() -> Result<()> {
    let path = token_path();
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("restrict {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// Run the OAuth desktop loopback flow. This blocks until the browser returns
/// to the local callback or the attempt times out.
pub fn link_account(config: &OAuthConfig) -> Result<DriveAccount> {
    let listener = TcpListener::bind(("127.0.0.1", 53682))
        .context("another Google Drive link is waiting, or port 53682 is already in use")?;
    listener
        .set_nonblocking(true)
        .context("configure the Google Drive callback listener")?;

    let state = Uuid::new_v4().simple().to_string();
    let url = auth_url(config, &state)?;
    open::that_detached(&url).context("open the Google Drive authorization page")?;

    let code = wait_for_code(&listener, &state, Duration::from_secs(180))?;
    let (token, about) = run(async move {
        let mut client = Client::new(
            config.client_id.clone(),
            config.client_secret.clone(),
            config.redirect_uri.clone(),
            "",
            "",
        );
        let token = client
            .get_access_token(&code, &state)
            .await
            .context("exchange the Google Drive authorization code")?;
        let about = About::new(client)
            .get()
            .await
            .ok()
            .map(|response| response.body);
        Ok::<_, anyhow::Error>((token, about))
    })?;

    if token.refresh_token.is_empty() {
        bail!(
            "Google did not return a refresh token. Remove Slate from myaccount.google.com/permissions, then link again."
        );
    }

    let user = about.and_then(|about| about.user);
    let account = DriveAccount {
        refresh_token: token.refresh_token,
        email: user
            .as_ref()
            .map(|user| user.email_address.clone())
            .unwrap_or_default(),
        name: user
            .as_ref()
            .map(|user| user.display_name.clone())
            .unwrap_or_default(),
    };
    save_account(&account)?;
    Ok(account)
}

fn auth_url(config: &OAuthConfig, state: &str) -> Result<String> {
    let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("response_type", "code")
        .append_pair(
            "scope",
            &format!("{DRIVE_READONLY_SCOPE} {DRIVE_FILE_SCOPE}"),
        )
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("include_granted_scopes", "true")
        .append_pair("state", state);
    Ok(url.into())
}

fn wait_for_code(
    listener: &TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> Result<String> {
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                let mut buffer = [0_u8; 8192];
                let read = stream.read(&mut buffer).unwrap_or(0);
                if read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..read]);
                let Some(target) = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                else {
                    respond(&mut stream, "400 Bad Request", "Invalid callback.");
                    continue;
                };
                let Ok(url) = Url::parse(&format!("http://127.0.0.1{target}")) else {
                    respond(&mut stream, "400 Bad Request", "Invalid callback.");
                    continue;
                };
                if url.path() != CALLBACK_PATH {
                    respond(&mut stream, "404 Not Found", "Not found.");
                    continue;
                }

                let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
                if let Some(error) = query.get("error") {
                    respond(
                        &mut stream,
                        "400 Bad Request",
                        "Authorization was cancelled.",
                    );
                    bail!("Google Drive authorization was cancelled: {error}");
                }
                if query.get("state") != Some(&expected_state.to_string()) {
                    respond(
                        &mut stream,
                        "400 Bad Request",
                        "Authorization state did not match.",
                    );
                    bail!("Google Drive authorization state did not match");
                }
                let Some(code) = query.get("code") else {
                    respond(
                        &mut stream,
                        "400 Bad Request",
                        "Authorization code was missing.",
                    );
                    bail!("Google Drive authorization code was missing");
                };

                respond(
                    &mut stream,
                    "200 OK",
                    "<!doctype html><meta charset=\"utf-8\"><title>Slate</title>\
                     <body style=\"font-family:-apple-system,system-ui,sans-serif;padding:48px\">\
                     <h1 style=\"font-size:20px\">Authorization received</h1>\
                     <p>Return to Slate to finish linking Google Drive.</p></body>",
                );
                return Ok(code.clone());
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    bail!("Google Drive authorization timed out");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error).context("accept the Google Drive callback"),
        }
    }
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn run<T, F>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("start the Google Drive runtime")?
        .block_on(future)
}

async fn authorized_client(
    config: &OAuthConfig,
    account: &DriveAccount,
) -> Result<(Client, String)> {
    let mut client = Client::new(
        config.client_id.clone(),
        config.client_secret.clone(),
        config.redirect_uri.clone(),
        "",
        account.refresh_token.clone(),
    );
    client.set_auto_access_token_refresh(true);
    let token = client
        .refresh_access_token()
        .await
        .context("refresh the Google Drive access token")?;
    Ok((client, token.access_token))
}

pub fn list_folder(
    config: &OAuthConfig,
    account: &DriveAccount,
    folder_id: &str,
) -> Result<Vec<DriveEntry>> {
    let config = config.clone();
    let account = account.clone();
    let folder_id = folder_id.to_string();
    run(async move {
        let (client, _) = authorized_client(&config, &account).await?;
        list_with_client(&Files::new(client), &folder_id).await
    })
}

async fn list_with_client(files: &Files, folder_id: &str) -> Result<Vec<DriveEntry>> {
    let query = format!("'{folder_id}' in parents and {FOLDER_QUERY}");
    let response = files
        .list_all(
            "user",
            "",
            true,
            "",
            false,
            "folder,name",
            &query,
            "",
            true,
            false,
            "",
        )
        .await
        .context("list the Google Drive folder")?;
    Ok(response
        .body
        .into_iter()
        .filter_map(entry_from_file)
        .collect())
}

pub fn collect_folder_images(
    config: &OAuthConfig,
    account: &DriveAccount,
    folder_id: &str,
) -> Result<Vec<DriveEntry>> {
    let config = config.clone();
    let account = account.clone();
    let folder_id = folder_id.to_string();
    run(async move {
        let (client, _) = authorized_client(&config, &account).await?;
        let files = Files::new(client);
        let mut pending = vec![(folder_id, 0_usize)];
        let mut images = Vec::new();
        while let Some((folder_id, depth)) = pending.pop() {
            if depth > 16 {
                continue;
            }
            let query = format!("'{folder_id}' in parents and {FOLDER_QUERY}");
            let response = files
                .list_all(
                    "user",
                    "",
                    true,
                    "",
                    false,
                    "folder,name",
                    &query,
                    "",
                    true,
                    false,
                    "",
                )
                .await
                .with_context(|| format!("list Drive folder {folder_id}"))?;
            for file in response.body {
                let Some(entry) = entry_from_file(file) else {
                    continue;
                };
                if entry.is_folder {
                    pending.push((entry.id, depth + 1));
                } else {
                    images.push(entry);
                }
            }
        }
        images.sort_by_cached_key(|entry| entry.name.to_lowercase());
        Ok(images)
    })
}

pub fn download_images(
    config: &OAuthConfig,
    account: &DriveAccount,
    entries: Vec<DriveEntry>,
    directory: &Path,
) -> Result<DownloadSummary> {
    let config = config.clone();
    let account = account.clone();
    let directory = directory.to_path_buf();
    run(async move {
        let (_, access_token) = authorized_client(&config, &account).await?;
        let http = reqwest::Client::new();
        let mut summary = DownloadSummary::default();
        for entry in entries {
            let folder = directory.join(safe_component(&entry.id));
            if let Err(error) = tokio::fs::create_dir_all(&folder).await {
                summary.errors.push(format!("{}: {error}", entry.name));
                continue;
            }
            let path = folder.join(local_file_name(&entry));
            match download_one(&http, &access_token, &entry.id, &path).await {
                Ok(()) => summary.paths.push(path),
                Err(error) => summary.errors.push(format!("{}: {error}", entry.name)),
            }
        }
        Ok(summary)
    })
}

async fn download_one(
    http: &reqwest::Client,
    access_token: &str,
    file_id: &str,
    path: &Path,
) -> Result<()> {
    let url = format!("{DOWNLOAD_BASE}/{file_id}?alt=media&supportsAllDrives=true");
    let mut response = http
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .context("start the Drive download")?;
    if !response.status().is_success() {
        bail!("Drive returned HTTP {}", response.status());
    }

    let mut file = tokio::fs::File::create(path)
        .await
        .context("create the local Drive download")?;
    while let Some(chunk) = response.chunk().await.context("read the Drive download")? {
        file.write_all(&chunk)
            .await
            .context("write the local Drive download")?;
    }
    file.flush()
        .await
        .context("flush the local Drive download")?;
    Ok(())
}

pub fn upload_pdf(
    config: &OAuthConfig,
    account: &DriveAccount,
    folder_id: &str,
    path: &Path,
    requested_name: &str,
) -> Result<UploadedFile> {
    let config = config.clone();
    let account = account.clone();
    let folder_id = if folder_id.is_empty() {
        "root".to_string()
    } else {
        folder_id.to_string()
    };
    let path = path.to_path_buf();
    let requested_name = requested_name.to_string();
    run(async move {
        let (client, access_token) = authorized_client(&config, &account).await?;
        let files = Files::new(client);
        let query = format!("'{folder_id}' in parents and {FOLDER_QUERY}");
        let existing = files
            .list_all(
                "user",
                "",
                true,
                "",
                false,
                "folder,name",
                &query,
                "",
                true,
                false,
                "",
            )
            .await
            .context("list the Drive export folder")?;
        let names: Vec<String> = existing.body.into_iter().map(|file| file.name).collect();
        let name = unique_name(&names, &requested_name);

        let length = tokio::fs::metadata(&path)
            .await
            .context("read the PDF for Drive upload")?
            .len();
        let body = json!({
            "name": name,
            "mimeType": "application/pdf",
            "parents": [folder_id],
        });
        let http = reqwest::Client::new();
        let response = http
            .post(format!(
                "{UPLOAD_BASE}?uploadType=resumable&supportsAllDrives=true&fields=id%2Cname"
            ))
            .bearer_auth(&access_token)
            .header("X-Upload-Content-Type", "application/pdf")
            .header("X-Upload-Content-Length", length.to_string())
            .json(&body)
            .send()
            .await
            .context("start the Drive upload")?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            bail!("Drive upload start failed: {status} {text}");
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .context("Drive did not return an upload URL")?
            .to_str()
            .context("Drive returned an invalid upload URL")?
            .to_string();

        let file = tokio::fs::File::open(&path)
            .await
            .context("open the PDF for Drive upload")?;
        let stream = ReaderStream::new(file);
        let response = http
            .put(location)
            .bearer_auth(&access_token)
            .header(reqwest::header::CONTENT_TYPE, "application/pdf")
            .header(reqwest::header::CONTENT_LENGTH, length.to_string())
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
            .context("upload the PDF to Drive")?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            bail!("Drive upload failed: {status} {text}");
        }

        #[derive(Deserialize)]
        struct Created {
            id: String,
            name: String,
        }
        let created: Created = response
            .json()
            .await
            .context("read the Drive upload result")?;
        Ok(UploadedFile {
            url: format!("https://drive.google.com/file/d/{}/view", created.id),
            id: created.id,
            name: created.name,
        })
    })
}

fn entry_from_file(file: File) -> Option<DriveEntry> {
    let is_folder = file.mime_type == FOLDER_MIME;
    let is_image = !is_folder
        && (file.mime_type.starts_with("image/") || import::is_supported(Path::new(&file.name)));
    if !is_folder && !is_image {
        return None;
    }
    Some(DriveEntry {
        id: file.id,
        name: file.name,
        mime_type: file.mime_type,
        size: u64::try_from(file.size).unwrap_or(0),
        is_folder,
    })
}

fn local_file_name(entry: &DriveEntry) -> String {
    let name = safe_file_name(&entry.name);
    if Path::new(&name).extension().is_some() {
        return name;
    }
    let extension = match entry.mime_type.as_str() {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/tiff" => Some("tiff"),
        "image/gif" => Some("gif"),
        "image/bmp" => Some("bmp"),
        "image/heic" | "image/heif" => Some("heic"),
        "image/avif" => Some("avif"),
        _ => None,
    };
    match extension {
        Some(extension) => format!("{name}.{extension}"),
        None => name,
    }
}

fn unique_name(existing: &[String], requested: &str) -> String {
    if !existing
        .iter()
        .any(|name| name.eq_ignore_ascii_case(requested))
    {
        return requested.to_string();
    }
    let (stem, extension) = match requested.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            (stem, Some(extension))
        }
        _ => (requested, None),
    };
    for index in 2..10_000 {
        let candidate = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        if !existing
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
    }
    requested.to_string()
}

fn safe_file_name(value: &str) -> String {
    let name = value.rsplit(['/', '\\']).next().unwrap_or(value).trim();
    let mut safe = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_control()
            || matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        {
            safe.push('-');
        } else {
            safe.push(character);
        }
    }
    let safe = safe
        .trim_matches(|character: char| character == '.' || character.is_whitespace())
        .to_string();
    if safe.is_empty() {
        "image".to_string()
    } else {
        safe
    }
}

fn safe_component(value: &str) -> String {
    let mut safe = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            safe.push(character);
        } else {
            safe.push('-');
        }
    }
    if safe.is_empty() {
        "drive-file".to_string()
    } else {
        safe
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_url_carries_offline_access_and_state() {
        let config = OAuthConfig {
            client_id: "client".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: REDIRECT_URI.to_string(),
        };
        let url = Url::parse(&auth_url(&config, "state-123").unwrap()).unwrap();
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(
            query.get("access_type").map(String::as_str),
            Some("offline")
        );
        assert_eq!(query.get("prompt").map(String::as_str), Some("consent"));
        assert_eq!(query.get("state").map(String::as_str), Some("state-123"));
        assert!(query["scope"].contains(DRIVE_READONLY_SCOPE));
        assert!(query["scope"].contains(DRIVE_FILE_SCOPE));
    }

    #[test]
    fn names_are_made_unique_without_overwriting() {
        let existing = vec!["contact-sheet.pdf".to_string()];
        assert_eq!(
            unique_name(&existing, "contact-sheet.pdf"),
            "contact-sheet-2.pdf"
        );
        assert_eq!(unique_name(&existing, "another.pdf"), "another.pdf");
    }

    #[test]
    fn unsafe_file_names_are_flattened() {
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("a:b?.jpg"), "a-b-.jpg");
        assert_eq!(safe_file_name("..."), "image");
    }

    #[test]
    fn drive_entries_keep_images_and_folders() {
        let folder = File {
            id: "folder".to_string(),
            name: "Photos".to_string(),
            mime_type: FOLDER_MIME.to_string(),
            ..File::default()
        };
        let image = File {
            id: "image".to_string(),
            name: "walk.jpg".to_string(),
            mime_type: "application/octet-stream".to_string(),
            size: 42,
            ..File::default()
        };
        let text = File {
            id: "text".to_string(),
            name: "notes.txt".to_string(),
            mime_type: "text/plain".to_string(),
            ..File::default()
        };

        assert!(entry_from_file(folder).unwrap().is_folder);
        let image = entry_from_file(image).unwrap();
        assert_eq!(image.size, 42);
        assert!(entry_from_file(text).is_none());

        let extensionless = entry_from_file(File {
            id: "extensionless".to_string(),
            name: "walk".to_string(),
            mime_type: "image/jpeg".to_string(),
            ..File::default()
        })
        .unwrap();
        assert_eq!(local_file_name(&extensionless), "walk.jpg");
    }
}
