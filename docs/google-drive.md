# Google Drive

Drive import and PDF export are optional. Slate uses a Google Cloud **Desktop
OAuth client** with PKCE. The public client ID is bundled into official builds.
Google currently still requires a client secret at the token endpoint for
Desktop clients, so official builds use a **token broker** that holds the secret.
A local `client_secret` works for development.

## Create the OAuth client

1. Open the [Google Cloud credentials page](https://console.cloud.google.com/apis/credentials).
2. Create or select a project.
3. Open **APIs & Services → Library**, search for **Google Drive API**, and enable it.
4. Open **APIs & Services → OAuth consent screen**.
   - Choose **External** while testing.
   - Add an app name, support email, and developer email.
   - Add the scopes `.../auth/drive.readonly` and `.../auth/drive.file`.
   - Add your Google account under **Test users**.
5. Open **Credentials → Create credentials → OAuth client ID**.
6. Choose **Desktop app** and create it.
7. Copy the client ID. Copy the client secret only if your client requires it.

Desktop clients use a loopback redirect. Slate listens on:

```text
http://127.0.0.1:53682/oauth2callback
```

You do not need to register that URI manually for a Desktop client.

## Configure Slate

Create `google-oauth.json` in Slate's application-data folder:

```json
{
  "client_id": "YOUR_CLIENT_ID"
}
```

PKCE uses a one-time verifier. Google still asks Desktop clients for a client
secret at the token endpoint, so choose one of these:

- **Official build:** point `token_broker` at the wwwtom API broker. The Worker
  holds the secret.
- **Local development:** add `client_secret` to this file. It stays local.

Official build configuration:

```json
{
  "client_id": "YOUR_CLIENT_ID",
  "token_broker": "https://api.tom.so"
}
```

`https://api.tom.so` hosts the generic broker at `/oauth/google/token` and
`/oauth/google/refresh`.

Local development configuration:

```json
{
  "client_id": "YOUR_CLIENT_ID",
  "client_secret": "YOUR_CLIENT_SECRET"
}
```

Default locations:

| Platform | Path |
| --- | --- |
| macOS | `~/Library/Application Support/slate-desktop/google-oauth.json` |
| Linux | `${XDG_DATA_HOME:-~/.local/share}/slate-desktop/google-oauth.json` |
| Windows | `%APPDATA%\slate-desktop\google-oauth.json` |

If `SLATE_DATA_DIR` is set, Slate uses `$SLATE_DATA_DIR/google-oauth.json`.

For development, you can set `SLATE_GOOGLE_CLIENT_ID`,
`SLATE_GOOGLE_CLIENT_SECRET`, and `SLATE_GOOGLE_TOKEN_BROKER`. Official builds
compile the public client ID and broker URL from the
`SLATE_GOOGLE_CLIENT_ID` and `SLATE_GOOGLE_TOKEN_BROKER` repository variables.
Slate never compiles a client secret.

## Token broker contract

The broker is a small HTTPS Worker with two JSON endpoints:

```text
POST /oauth/google/token
{ "code", "code_verifier", "redirect_uri", "client_id" }

POST /oauth/google/refresh
{ "refresh_token", "client_id" }
```

Both return:

```json
{ "access_token": "…", "refresh_token": "…" }
```

The Worker holds `SLATE_GOOGLE_CLIENT_ID` and
`SLATE_GOOGLE_CLIENT_SECRET`, adds the secret to the Google token request, and
never logs codes, tokens, or the secret. It must reject GET and invalid input.

Save the file, then press **+** in the Folders pane and choose **Google Drive**
(or **File → Import from Google Drive**). The first use links the account: the
browser opens, the callback returns to the app, and the Drive browser opens.
**File → Unlink Google Drive** removes the stored refresh token;
revoke the grant at
[myaccount.google.com/permissions](https://myaccount.google.com/permissions) to
remove Slate from the account entirely.

## What Slate stores

- `google-drive.json` holds the refresh token and account label. On Unix it is
  written with mode `0600`.
- Imported originals are mirrored into `Google Drive/` in Slate's
  application-data folder, following Drive's own folder names (for example
  `Google Drive/My Drive/Shoots/Walk/`). The mirrored folder opens as an
  ordinary folder collection with its own `collection.slate`, and the Folders
  pane shows the mirror under Google Drive. Importing a folder again only
  downloads files that are not already there; a different file with the same
  name gets a numeric suffix instead of overwriting.
- PDF exports go through a temporary file and stream to the Drive folder you
  choose. Slate never overwrites an existing Drive file; duplicate names get a
  numeric suffix.

## Scopes

| Scope | Use |
| --- | --- |
| `drive.readonly` | Browse folders, list images, download selected originals |
| `drive.file` | Upload exported PDFs created by Slate |

## Testing-mode tokens

An External app in **Testing** issues refresh tokens that expire after seven
days. Publish the app (and complete Google verification if required) or use an
Internal app in a Google Workspace for long-lived tokens.

## Troubleshooting

- **Google Drive is not configured**: the config file is missing or invalid.
  Check the path shown in the app's Drive notice.
- **redirect_uri_mismatch**: use an OAuth client of type **Desktop app**, not
  **Web application**.
- **access_denied**: add the signing-in account under **Test users**.
- **Port 53682 is already in use**: close the other app or stop the previous
  link attempt, then try again.
- **No refresh token**: remove Slate's access at
  [myaccount.google.com/permissions](https://myaccount.google.com/permissions)
  and link again.
- **`client_secret is missing`**: this Google client requires a secret at the
  token endpoint. Add `client_secret` for local development, or set
  `token_broker` to the official Worker.
- **Repeated 401 responses**: unlink and relink the account. A testing-mode
  refresh token may have expired.
