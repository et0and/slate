# Google Drive

Drive import and PDF export are optional. Slate uses a Google Cloud **Desktop
OAuth client**; no client secret ships in the repository.

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
7. Copy the client ID and client secret.

Desktop clients use a loopback redirect. Slate listens on:

```text
http://127.0.0.1:53682/oauth2callback
```

You do not need to register that URI manually for a Desktop client.

## Configure Slate

Create `google-oauth.json` in Slate's application-data folder:

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

For development, you can set `SLATE_GOOGLE_CLIENT_ID` and
`SLATE_GOOGLE_CLIENT_SECRET` instead. Release builds may bake the same values in
at build time; the local file still wins.

Save the file, then choose **Drive → Link Google Drive**. The browser opens, and
the callback returns to the app. Drive → Unlink removes the stored refresh token;
revoke the grant at
[myaccount.google.com/permissions](https://myaccount.google.com/permissions) to
remove Slate from the account entirely.

## What Slate stores

- `google-drive.json` holds the refresh token and account label. On Unix it is
  written with mode `0600`.
- Imported originals are downloaded to `Drive imports/<folder>/<file-id>/` next
  to the `.slate` project, or to Slate's application-data folder for an unsaved
  project. The project references those paths; no copies are embedded in the
  `.slate` file.
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
- **Repeated 401 responses**: unlink and relink the account. A testing-mode
  refresh token may have expired.
