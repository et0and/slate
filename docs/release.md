# Releases

Slate uses [semantic-release](https://semantic-release.gitbook.io/semantic-release/)
on `main`, the same commit-driven model as the wwwtom monorepo. The workflow
turns Conventional Commits into versions, tags, changelogs and GitHub Releases;
the macOS, Windows and Linux downloads are built from the tag and attached to
that release as `Slate-macos-universal.dmg`, `Slate-windows-x86_64.exe` and
`Slate-linux-x86_64.AppImage`. They are direct downloads, not archives, and keep
the same names every release so `releases/latest/download/<name>` links stay
valid.

## How a release runs

Every push to `main`:

1. Reads commits since the last `v*` tag.
2. Chooses the next version:
   - `feat:` — minor
   - `fix:`, `perf:` — patch
   - `BREAKING CHANGE:` in the body, or `feat!:` / `fix!:` — major
   - `chore:`, `ci:`, `docs:`, `refactor:`, `test:` — no release unless breaking
3. Updates `Cargo.toml`, `Cargo.lock`, `packaging/macos/Info.plist` and
   `CHANGELOG.md`.
4. Commits `chore(release): <version> [skip ci]`, tags `v<version>` and creates
   the GitHub Release with generated notes.
5. Builds the macOS disk image, the Windows executable and the Linux AppImage
   from the tag, then uploads all three assets to the release.

### First release

semantic-release normally starts at `v1.0.0` when no previous tag exists. Slate
starts at `v0.0.1`, so bootstrap the first tag once:

```sh
git tag -a v0.0.1 -m "Slate 0.0.1" <merge-commit>
git push origin v0.0.1
gh workflow run release.yml -f tag=v0.0.1
```

The manual run builds the artifacts from the tag and creates the GitHub Release.
After `v0.0.1`, every push to `main` releases automatically: `feat` is minor and
`fix`/`perf` is patch.

## Commit rules

Release notes and version numbers come from commit messages. Use the same
format as the rest of the repository:

```text
feat(app): add collapsible side panes
fix(drive): keep the OAuth callback listener alive on bad requests
perf(core): cut the 10k project working set below 100 MB
feat(app)!: replace the project format
```

When a pull request is squashed, the pull request title becomes the commit
subject, so the title must be conventional too. Merge commits are ignored by the
analyser; the commits inside the merge are what matter.

## Rebuilding a release

If a runner fails after the tag exists, open **Actions → Release → Run
workflow**, enter the existing tag (for example `v1.2.3`) and run it. The build
jobs check out that tag and upload the artifacts again with `--clobber`. A
manual run with an empty tag analyses `main` as usual.

## macOS signing and notarization

A release is ad-hoc signed and **not notarized** until the six repository
secrets below exist. The workflow prints a warning in that state. Once the
secrets are present it signs with a Developer ID Application certificate,
notarizes the app, staples the ticket and verifies it with Gatekeeper, then
wraps it in a disk image that is signed, notarized and stapled in turn.

### What you need

- An active [Apple Developer Program](https://developer.apple.com/programs/)
  membership. Developer ID signing and notarization are paid-account features.
- A **Developer ID Application** certificate, with its private key, in your
  login keychain. An Apple Distribution certificate is not a substitute.
- The 10-character Apple **Team ID**.
- An **app-specific password** for the Apple ID that owns the team, created at
  [appleid.apple.com](https://appleid.apple.com/account/manage). Normal Apple ID
  passwords do not work with `notarytool`.

Confirm the certificate is present:

```sh
security find-identity -v -p codesigning
```

The identity looks like:

```text
Developer ID Application: Tom Hackshaw (ABCDE12345)
```

### Export the certificate

In **Keychain Access → My Certificates**, select the Developer ID Application
certificate (not just the public key), choose **Export Items**, and save a
`.p12` with a password. The file must contain the private key.

Encode it for GitHub:

```sh
base64 -i developer-id.p12 -o /tmp/slate-developer-id.p12.b64
```

### Add repository secrets

Run these from the Slate checkout. Use the same password you set when exporting
the `.p12`.

```sh
gh secret set MACOS_SIGNING_CERTIFICATE < /tmp/slate-developer-id.p12.b64
gh secret set MACOS_SIGNING_CERTIFICATE_PASSWORD
gh secret set MACOS_SIGNING_IDENTITY
gh secret set APPLE_ID
gh secret set APPLE_APP_SPECIFIC_PASSWORD
gh secret set APPLE_TEAM_ID
```

| Secret | Value |
| --- | --- |
| `MACOS_SIGNING_CERTIFICATE` | Base64 of the `.p12` containing the certificate and private key |
| `MACOS_SIGNING_CERTIFICATE_PASSWORD` | Password chosen when exporting the `.p12` |
| `MACOS_SIGNING_IDENTITY` | Full identity, for example `Developer ID Application: Tom Hackshaw (ABCDE12345)` |
| `APPLE_ID` | Apple ID email for the developer account |
| `APPLE_APP_SPECIFIC_PASSWORD` | App-specific password for that Apple ID |
| `APPLE_TEAM_ID` | 10-character team identifier |

Delete the temporary base64 file after setting the secret:

```sh
rm /tmp/slate-developer-id.p12.b64
```

Never commit the `.p12`, its password, the app-specific password or the Team
ID. The workflow imports the certificate into a temporary keychain and deletes
the keychain after the build.

### What the workflow does

1. Builds the arm64 and x86_64 binaries and merges them with `lipo`.
2. Sets `CFBundleShortVersionString` and a numeric `CFBundleVersion`.
3. Signs the executable and the app bundle with
   `codesign --options runtime --timestamp` (hardened runtime, no `--deep`).
4. Zips the app with `ditto` and submits it to `xcrun notarytool submit --wait`.
5. Staples the accepted ticket with `xcrun stapler staple`, validates it, and
   checks the result with `spctl --assess`.
6. Copies the stapled app and an `/Applications` link into
   `Slate-macos-universal.dmg` with `hdiutil`.
7. Signs the disk image, notarizes and staples it, and checks it with
   `spctl --assess --type open`.
8. Uploads the disk image to the GitHub Release.

Without the signing secrets the app is ad-hoc signed and the disk image is
unsigned.

## FFmpeg

Video import needs FFmpeg's demuxers, decoders and scaler. Every release
links them statically, so no download depends on an FFmpeg on the user's
machine:

- macOS and Linux build with `--features bundled-ffmpeg`. `ffmpeg-sys-next`
  clones FFmpeg's release branch and compiles it during `cargo build`, once per
  architecture (the macOS job builds arm64 and x86_64). `nasm` assembles the x86
  code. The feature enables `build-portable`, so FFmpeg is not tuned with
  `-march=native` for the CI machine. The x86_64 half is a cross build, which
  needs the `macos` → `darwin` fix in `vendor/ffmpeg-sys-next`.
- Windows installs `ffmpeg[core,avcodec,avformat,swscale]:x64-windows-static-md`
  with the runner's vcpkg, caches vcpkg's binary output with `actions/cache`, and
  builds with `--features static-ffmpeg`.

FFmpeg is LGPL 2.1 or later in this configuration (no `--enable-gpl` parts),
which Slate's GPLv3 licence can include.

## Windows and Linux packages

- Windows ships the release `slate-desktop.exe` itself as
  `Slate-windows-x86_64.exe`. It is self-contained (fonts and interface icons are
  compiled in) and is not code-signed, so SmartScreen warns on first launch.
- Linux ships `Slate-linux-x86_64.AppImage`: an AppDir with the binary,
  `packaging/linux/slate.desktop` and the icon, compressed with `mksquashfs`
  and appended to the official AppImage type2 runtime. The runtime is
  downloaded from a pinned `AppImage/type2-runtime` release and checked against
  its SHA-256 before use. The AppImage still uses the host's graphics, font and
  input libraries, and it needs FUSE to mount itself (or run it with
  `--appimage-extract-and-run`).

Slate does not use the App Sandbox, so the Developer ID build needs no sandbox
entitlements. If a future feature needs an entitlement, add a
`packaging/macos/Slate.entitlements` plist and pass it to `codesign`; do not add
entitlements speculatively.

### Verify a downloaded build

```sh
codesign --verify --deep --strict --verbose=2 /Applications/Slate.app
spctl --assess --type execute --verbose=4 /Applications/Slate.app
xcrun stapler validate /Applications/Slate.app
lipo -info /Applications/Slate.app/Contents/MacOS/slate-desktop
```

`spctl` should say `source=Notarized Developer ID`. If it says
`source=Unnotarized Developer ID`, the staple step or the upload used the wrong
build.

### App Store Connect API key alternative

Instead of an Apple ID and app-specific password, `notarytool` can authenticate
with an App Store Connect API key. Create a **Team Key** with access to
notarization in App Store Connect, download the `.p8` once, and store these
secrets instead:

| Secret | Value |
| --- | --- |
| `APPLE_API_KEY` | Base64 of the `AuthKey_<KEY_ID>.p8` file |
| `APPLE_API_KEY_ID` | Key ID |
| `APPLE_API_ISSUER_ID` | Issuer ID |

Then replace the `notarytool` arguments in `.github/workflows/release.yml`:

```sh
xcrun notarytool submit "$RUNNER_TEMP/Slate-notarize.zip" \
  --key "$RUNNER_TEMP/AuthKey.p8" \
  --key-id "$APPLE_API_KEY_ID" \
  --issuer "$APPLE_API_ISSUER_ID" \
  --wait
```

API keys are better for long-lived CI because they are not tied to an Apple ID
password and can be revoked independently.

### Troubleshooting

- **`notarytool` rejects the app:** check that the certificate is Developer ID
  Application, the build used `--options runtime`, and `--timestamp` was set.
- **`codesign` cannot find the identity:** the `.p12` did not include the private
  key, or `MACOS_SIGNING_IDENTITY` does not match `security find-identity`
  exactly.
- **Semantic release cannot push:** branch protection on `main` must allow the
  GitHub Actions token to push the release commit and tag, or use a fine-grained
  token with `contents: write` in place of `secrets.GITHUB_TOKEN`.
- **Notarization times out:** rerun the workflow with the existing tag; the app
  build is reproducible from the tag.
- **First release is unexpected:** check `git tag --list 'v*'`. With no previous
  tag, semantic-release starts at `1.0.0`, so the `v0.0.1` bootstrap tag must
  exist before the first automatic release.
