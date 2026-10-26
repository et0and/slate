//! Google Drive OAuth, account persistence and file transfer.
//!
//! `google-drive` owns OAuth and folder listing. Transfers use a streaming
//! request so a 10,000-photo export never has to hold a whole PDF in memory.
//!
//! The public client ID may be compiled into an official build. Client
//! secrets and refresh tokens are never compiled into the binary.

use std::collections::HashMap;
use std::future::Future;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use google_drive::Client;
use google_drive::about::About;
use google_drive::files::Files;
#[cfg(test)]
use google_drive::types::File;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use url::Url;
use uuid::Uuid;

use slate_core::import;
use slate_core::store::data_directory;

const REDIRECT_URI: &str = "http://127.0.0.1:53682/oauth2callback";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
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
    pub client_secret: Option<String>,
    pub token_broker: Option<String>,
    pub redirect_uri: String,
}

impl OAuthConfig {
    fn secret(&self) -> Option<&str> {
        self.client_secret
            .as_deref()
            .map(str::trim)
            .filter(|secret| !secret.is_empty())
    }

    fn broker(&self) -> Option<&str> {
        self.token_broker
            .as_deref()
            .map(str::trim)
            .filter(|broker| !broker.is_empty())
    }
}

fn broker_url(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Deserialize)]
struct OAuthConfigFile {
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    token_broker: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthToken {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DriveEntry {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub is_folder: bool,
    pub thumbnail_link: String,
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

/// A Drive image and the folder it mirrors into, relative to the mirrored
/// Drive folder that was imported.
#[derive(Debug, Clone)]
pub struct MirrorFile {
    pub entry: DriveEntry,
    pub folder: PathBuf,
}

#[derive(Debug, Default)]
pub struct DownloadSummary {
    /// Every local copy, whether it was downloaded now or already present.
    pub paths: Vec<PathBuf>,
    /// How many of `paths` were already on disk from an earlier import.
    pub present: usize,
    pub errors: Vec<String>,
}

/// Drive imports mirror Drive's folder hierarchy here, so an imported Drive
/// folder is an ordinary folder collection with its own `collection.slate`.
pub fn mount_directory() -> PathBuf {
    data_directory().join("Google Drive")
}

/// The local mirror of a Drive folder, from its path of folder names
/// starting at "My Drive".
pub fn mirror_folder<'a>(names: impl IntoIterator<Item = &'a str>) -> PathBuf {
    names.into_iter().fold(mount_directory(), |path, name| {
        path.join(safe_file_name(name))
    })
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

/// Resolve an OAuth client from the runtime environment, a local config file,
/// or the public client ID compiled into an official build. The client secret is
/// optional and never compiled into a build.
pub fn oauth_config() -> Result<OAuthConfig> {
    if let Ok(client_id) = std::env::var("SLATE_GOOGLE_CLIENT_ID") {
        let client_id = client_id.trim();
        if !client_id.is_empty() {
            let client_secret = std::env::var("SLATE_GOOGLE_CLIENT_SECRET")
                .ok()
                .map(|secret| secret.trim().to_string())
                .filter(|secret| !secret.is_empty());
            let token_broker = broker_url(std::env::var("SLATE_GOOGLE_TOKEN_BROKER").ok());
            return Ok(OAuthConfig {
                client_id: client_id.to_string(),
                client_secret,
                token_broker,
                redirect_uri: REDIRECT_URI.to_string(),
            });
        }
    }

    let path = oauth_config_path();
    if path.is_file() {
        let data =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let file: OAuthConfigFile =
            serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))?;
        let client_id = file.client_id.trim();
        if !client_id.is_empty() {
            let client_secret = file
                .client_secret
                .map(|secret| secret.trim().to_string())
                .filter(|secret| !secret.is_empty());
            return Ok(OAuthConfig {
                client_id: client_id.to_string(),
                client_secret,
                token_broker: broker_url(file.token_broker),
                redirect_uri: REDIRECT_URI.to_string(),
            });
        }
    }

    if let Some(client_id) = option_env!("SLATE_GOOGLE_CLIENT_ID") {
        let client_id = client_id.trim();
        if !client_id.is_empty() {
            return Ok(OAuthConfig {
                client_id: client_id.to_string(),
                client_secret: None,
                token_broker: broker_url(
                    option_env!("SLATE_GOOGLE_TOKEN_BROKER").map(str::to_string),
                ),
                redirect_uri: REDIRECT_URI.to_string(),
            });
        }
    }

    bail!(
        "Google Drive is not configured. Add a Desktop OAuth client ID to {}.",
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
            "Save the client ID in {} as {{\"client_id\":\"…\"}}. Add client_secret for local testing, or token_broker (https://api.tom.so) for an official build.",
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
#[expect(
    clippy::unnecessary_wraps,
    reason = "the caller uses ? on every platform"
)]
const fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// Run the OAuth desktop loopback flow. This blocks until the browser returns
/// to the local callback or the attempt times out. PKCE protects the code
/// exchange; a client secret is optional.
pub fn link_account(config: &OAuthConfig) -> Result<DriveAccount> {
    if config.secret().is_none() && config.broker().is_none() {
        bail!(
            "Google requires a client secret for this Desktop client. Add client_secret to {} for local use, or token_broker for an official build.",
            oauth_config_path().display()
        );
    }

    let listener = TcpListener::bind(("127.0.0.1", 53682))
        .context("another Google Drive link is waiting, or port 53682 is already in use")?;
    listener
        .set_nonblocking(true)
        .context("configure the Google Drive callback listener")?;

    let state = Uuid::new_v4().simple().to_string();
    let verifier = code_verifier();
    let challenge = code_challenge(&verifier);
    let url = auth_url(config, &state, &challenge)?;
    open::that_detached(&url).context("open the Google Drive authorization page")?;

    let code = wait_for_code(&listener, &state, Duration::from_secs(180))?;
    let (token, about) = run(async move {
        let token = exchange_code(config, &code, &verifier).await?;
        let client = Client::new(
            config.client_id.clone(),
            config.secret().unwrap_or_default().to_string(),
            config.redirect_uri.clone(),
            token.access_token.clone(),
            token.refresh_token.clone().unwrap_or_default(),
        );
        let about = About::new(client)
            .get()
            .await
            .ok()
            .map(|response| response.body);
        Ok::<_, anyhow::Error>((token, about))
    })?;

    let refresh_token = token.refresh_token.unwrap_or_default();
    if refresh_token.is_empty() {
        bail!(
            "Google did not return a refresh token. Remove Slate from myaccount.google.com/permissions, then link again."
        );
    }

    let user = about.and_then(|about| about.user);
    let account = DriveAccount {
        refresh_token,
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

fn auth_url(config: &OAuthConfig, state: &str, challenge: &str) -> Result<String> {
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
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    Ok(url.into())
}

fn code_verifier() -> String {
    let first = Uuid::new_v4().simple().to_string();
    let second = Uuid::new_v4().simple().to_string();
    format!("{first}{second}")
}

fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

async fn exchange_code(config: &OAuthConfig, code: &str, verifier: &str) -> Result<OAuthToken> {
    if config.secret().is_none()
        && let Some(broker) = config.broker()
    {
        return post_broker(
            broker,
            "/oauth/google/token",
            &BrokerExchange {
                code,
                code_verifier: verifier,
                redirect_uri: &config.redirect_uri,
                client_id: &config.client_id,
            },
        )
        .await;
    }

    let mut params = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("client_id", config.client_id.clone()),
        ("redirect_uri", config.redirect_uri.clone()),
        ("code_verifier", verifier.to_string()),
    ];
    if let Some(secret) = config.secret() {
        params.push(("client_secret", secret.to_string()));
    }
    post_token(params).await
}

async fn refresh_access_token(config: &OAuthConfig, refresh_token: &str) -> Result<OAuthToken> {
    if config.secret().is_none()
        && let Some(broker) = config.broker()
    {
        return post_broker(
            broker,
            "/oauth/google/refresh",
            &BrokerRefresh {
                refresh_token,
                client_id: &config.client_id,
            },
        )
        .await;
    }

    let mut params = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("client_id", config.client_id.clone()),
    ];
    if let Some(secret) = config.secret() {
        params.push(("client_secret", secret.to_string()));
    }
    post_token(params).await
}

#[derive(Serialize)]
struct BrokerExchange<'a> {
    code: &'a str,
    code_verifier: &'a str,
    redirect_uri: &'a str,
    client_id: &'a str,
}

#[derive(Serialize)]
struct BrokerRefresh<'a> {
    refresh_token: &'a str,
    client_id: &'a str,
}

async fn post_broker(broker: &str, path: &str, body: &impl Serialize) -> Result<OAuthToken> {
    let url = format!("{}{}", broker.trim_end_matches('/'), path);
    let response = http_client()
        .post(url)
        .json(body)
        .send()
        .await
        .context("send the Google token broker request")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("read the Google token broker response")?;
    if !status.is_success() {
        bail!("Google token broker failed: {status} {body}");
    }
    serde_json::from_str(&body).context("parse the Google token broker response")
}

async fn post_token(params: Vec<(&'static str, String)>) -> Result<OAuthToken> {
    let response = http_client()
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .context("send the Google token request")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("read the Google token response")?;
    if !status.is_success() {
        if body.contains("client_secret is missing") {
            bail!(
                "Google rejected the PKCE exchange because it requires a client secret. Add client_secret to {} or use a server broker.",
                oauth_config_path().display()
            );
        }
        bail!("Google token request failed: {status} {body}");
    }
    serde_json::from_str(&body).context("parse the Google token response")
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

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Slate/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("Google Drive HTTP client")
}

async fn authorized_client(
    config: &OAuthConfig,
    account: &DriveAccount,
) -> Result<(Client, String)> {
    let token = refresh_access_token(config, &account.refresh_token).await?;
    let client = Client::new(
        config.client_id.clone(),
        config.secret().unwrap_or_default().to_string(),
        config.redirect_uri.clone(),
        token.access_token.clone(),
        account.refresh_token.clone(),
    );
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
        let (_, access_token) = authorized_client(&config, &account).await?;
        list_with_token(&http_client(), &access_token, &folder_id).await
    })
}

#[derive(Debug, Deserialize)]
struct DriveFileList {
    #[serde(default)]
    files: Vec<DriveFileJson>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: String,
}

#[derive(Debug, Deserialize)]
struct DriveFileJson {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "mimeType")]
    mime_type: String,
    #[serde(default)]
    size: Option<String>,
    #[serde(default, rename = "thumbnailLink")]
    thumbnail_link: String,
}

impl DriveFileJson {
    fn into_entry(self) -> Option<DriveEntry> {
        let is_folder = self.mime_type == FOLDER_MIME;
        let is_image = !is_folder
            && (self.mime_type.starts_with("image/")
                || import::is_supported(Path::new(&self.name)));
        if !is_folder && !is_image {
            return None;
        }
        Some(DriveEntry {
            id: self.id,
            name: self.name,
            mime_type: self.mime_type,
            size: self.size.and_then(|size| size.parse().ok()).unwrap_or(0),
            is_folder,
            thumbnail_link: self.thumbnail_link,
        })
    }
}

async fn list_with_token(
    http: &reqwest::Client,
    access_token: &str,
    folder_id: &str,
) -> Result<Vec<DriveEntry>> {
    let query = format!("'{folder_id}' in parents and {FOLDER_QUERY}");
    let mut page_token = String::new();
    let mut entries = Vec::new();
    loop {
        let mut request = http.get(DOWNLOAD_BASE).bearer_auth(access_token).query(&[
            ("q", query.as_str()),
            (
                "fields",
                "nextPageToken,files(id,name,mimeType,size,thumbnailLink)",
            ),
            ("pageSize", "1000"),
            ("orderBy", "folder,name"),
            ("supportsAllDrives", "true"),
            ("includeItemsFromAllDrives", "true"),
        ]);
        if !page_token.is_empty() {
            request = request.query(&[("pageToken", page_token.as_str())]);
        }
        let response = request
            .send()
            .await
            .context("list the Google Drive folder")?;
        if !response.status().is_success() {
            anyhow::bail!("Drive folder returned HTTP {}", response.status());
        }
        let page: DriveFileList = response
            .json()
            .await
            .context("decode the Google Drive folder")?;
        entries.extend(page.files.into_iter().filter_map(DriveFileJson::into_entry));
        page_token = page.next_page_token;
        if page_token.is_empty() {
            break;
        }
    }
    Ok(entries)
}

pub fn collect_folder_images(
    config: &OAuthConfig,
    account: &DriveAccount,
    folder_id: &str,
) -> Result<Vec<MirrorFile>> {
    let config = config.clone();
    let account = account.clone();
    let folder_id = folder_id.to_string();
    run(async move {
        let (_, access_token) = authorized_client(&config, &account).await?;
        let http = http_client();
        let mut pending = vec![(folder_id, PathBuf::new())];
        let mut images = Vec::new();
        while let Some((folder_id, folder)) = pending.pop() {
            if folder.components().count() > 16 {
                continue;
            }
            let entries = list_with_token(&http, &access_token, &folder_id)
                .await
                .with_context(|| format!("list Drive folder {folder_id}"))?;
            for entry in entries {
                if entry.is_folder {
                    let child = folder.join(safe_file_name(&entry.name));
                    pending.push((entry.id, child));
                } else {
                    images.push(MirrorFile {
                        entry,
                        folder: folder.clone(),
                    });
                }
            }
        }
        images.sort_by_cached_key(|file| (file.folder.clone(), file.entry.name.to_lowercase()));
        Ok(images)
    })
}

/// Fetch a Drive-provided thumbnail. Google signs these URLs, so try the
/// direct URL first and only fall back to the access token if needed.
pub fn fetch_thumbnail(
    config: &OAuthConfig,
    account: &DriveAccount,
    link: &str,
) -> Result<Vec<u8>> {
    let config = config.clone();
    let account = account.clone();
    let link = link.to_string();
    run(async move {
        let http = http_client();
        let direct = http
            .get(&link)
            .send()
            .await
            .context("start the Drive thumbnail request")?;
        if direct.status().is_success() {
            return Ok(direct.bytes().await?.to_vec());
        }
        let (_, access_token) = authorized_client(&config, &account).await?;
        let response = http
            .get(&link)
            .bearer_auth(access_token)
            .send()
            .await
            .context("start the authenticated Drive thumbnail request")?;
        if !response.status().is_success() {
            anyhow::bail!("Drive thumbnail returned HTTP {}", response.status());
        }
        Ok(response.bytes().await?.to_vec())
    })
}

/// Download `files` into the mirror rooted at `directory`. A file already
/// mirrored by an earlier import is reused, so importing a folder again only
/// fetches what changed; a different file with the same name gets a numbered
/// name instead of overwriting.
pub fn download_images(
    config: &OAuthConfig,
    account: &DriveAccount,
    files: Vec<MirrorFile>,
    directory: &Path,
) -> Result<DownloadSummary> {
    let config = config.clone();
    let account = account.clone();
    let directory = directory.to_path_buf();
    run(async move {
        let (_, access_token) = authorized_client(&config, &account).await?;
        let http = http_client();
        let mut summary = DownloadSummary::default();
        for MirrorFile { entry, folder } in files {
            let folder = directory.join(folder);
            if let Err(error) = tokio::fs::create_dir_all(&folder).await {
                summary.errors.push(format!("{}: {error}", entry.name));
                continue;
            }
            match local_copy(&folder, &entry) {
                LocalCopy::Present(path) => {
                    summary.present += 1;
                    summary.paths.push(path);
                }
                LocalCopy::Missing(path) => {
                    match download_one(&http, &access_token, &entry.id, &path).await {
                        Ok(()) => summary.paths.push(path),
                        Err(error) => summary.errors.push(format!("{}: {error}", entry.name)),
                    }
                }
            }
        }
        Ok(summary)
    })
}

enum LocalCopy {
    Present(PathBuf),
    Missing(PathBuf),
}

/// Find this Drive file's place in a mirror folder: the first name that is
/// either free or already holds a file of the same size.
fn local_copy(folder: &Path, entry: &DriveEntry) -> LocalCopy {
    let requested = local_file_name(entry);
    for index in 1..10_000 {
        let path = folder.join(numbered_name(&requested, index));
        match std::fs::metadata(&path) {
            Err(_) => return LocalCopy::Missing(path),
            Ok(metadata) if entry.size > 0 && metadata.len() == entry.size => {
                return LocalCopy::Present(path);
            }
            Ok(_) => {}
        }
    }
    LocalCopy::Missing(folder.join(format!("{}-{requested}", safe_component(&entry.id))))
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

    // Stream into a sibling `.part` file so an interrupted download never
    // looks like a finished photo in the mirror.
    let mut partial = path.as_os_str().to_owned();
    partial.push(".part");
    let partial = PathBuf::from(partial);
    let result = async {
        let mut file = tokio::fs::File::create(&partial)
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
        tokio::fs::rename(&partial, path)
            .await
            .context("finish the local Drive download")
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    result
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
        #[derive(Deserialize)]
        struct Created {
            id: String,
            name: String,
        }
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
        let http = http_client();
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

#[cfg(test)]
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
        thumbnail_link: file.thumbnail_link,
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
    (1..10_000)
        .map(|index| numbered_name(requested, index))
        .find(|candidate| {
            !existing
                .iter()
                .any(|name| name.eq_ignore_ascii_case(candidate))
        })
        .unwrap_or_else(|| requested.to_string())
}

/// `index` 1 is the name itself; later indices number the stem:
/// `walk.jpg`, `walk-2.jpg`, `walk-3.jpg`.
fn numbered_name(requested: &str, index: usize) -> String {
    if index == 1 {
        return requested.to_string();
    }
    match requested.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            format!("{stem}-{index}.{extension}")
        }
        _ => format!("{requested}-{index}"),
    }
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
    fn auth_url_carries_offline_access_pkce_and_state() {
        let config = OAuthConfig {
            client_id: "client".to_string(),
            client_secret: None,
            token_broker: None,
            redirect_uri: REDIRECT_URI.to_string(),
        };
        let challenge = code_challenge("abc");
        let url = Url::parse(&auth_url(&config, "state-123", &challenge).unwrap()).unwrap();
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(
            query.get("access_type").map(String::as_str),
            Some("offline")
        );
        assert_eq!(query.get("prompt").map(String::as_str), Some("consent"));
        assert_eq!(query.get("state").map(String::as_str), Some("state-123"));
        assert_eq!(
            query.get("code_challenge").map(String::as_str),
            Some("ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0")
        );
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert!(query["scope"].contains(DRIVE_READONLY_SCOPE));
        assert!(query["scope"].contains(DRIVE_FILE_SCOPE));
    }

    #[test]
    fn broker_exchange_uses_the_generic_contract() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).unwrap_or(0);
            sender
                .send(String::from_utf8_lossy(&buffer[..read]).into_owned())
                .unwrap();
            let body = r#"{"access_token":"access-token","refresh_token":"refresh-token"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let token = run(async {
            post_broker(
                &format!("http://{address}"),
                "/oauth/google/token",
                &BrokerExchange {
                    code: "code",
                    code_verifier: "verifier",
                    redirect_uri: REDIRECT_URI,
                    client_id: "client",
                },
            )
            .await
        })
        .unwrap();
        assert_eq!(token.access_token, "access-token");
        let request = receiver.recv().unwrap();
        assert!(request.starts_with("POST /oauth/google/token HTTP/1.1"));
        assert!(request.contains(r#""client_id":"client""#));
        server.join().unwrap();
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
    fn mirrors_reuse_matching_copies_and_never_overwrite() {
        let folder = std::env::temp_dir().join(format!("slate-mirror-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("walk.jpg"), b"four").unwrap();
        let entry = |size| DriveEntry {
            id: "id".to_string(),
            name: "walk.jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            size,
            is_folder: false,
            thumbnail_link: String::new(),
        };
        assert!(matches!(
            local_copy(&folder, &entry(4)),
            LocalCopy::Present(path) if path == folder.join("walk.jpg")
        ));
        assert!(matches!(
            local_copy(&folder, &entry(9)),
            LocalCopy::Missing(path) if path == folder.join("walk-2.jpg")
        ));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn mirror_folders_follow_drive_names_safely() {
        assert_eq!(
            mirror_folder(["My Drive", "Shoots/2026", ".."]),
            mount_directory()
                .join("My Drive")
                .join("2026")
                .join("image")
        );
    }

    #[test]
    fn unsafe_file_names_are_flattened() {
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("a:b?.jpg"), "a-b-.jpg");
        assert_eq!(safe_file_name("..."), "image");
    }

    #[test]
    fn reads_drive_thumbnails_and_sizes() {
        let page: DriveFileList = serde_json::from_str(
            r#"{
                "files": [{
                    "id": "photo-1",
                    "name": "photo.jpg",
                    "mimeType": "image/jpeg",
                    "size": "42000",
                    "thumbnailLink": "https://example.test/thumb"
                }]
            }"#,
        )
        .unwrap();
        let entry = page.files.into_iter().next().unwrap().into_entry().unwrap();
        assert_eq!(entry.size, 42_000);
        assert_eq!(entry.thumbnail_link, "https://example.test/thumb");
        assert!(!entry.is_folder);
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
