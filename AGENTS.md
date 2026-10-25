# Repository guide

Slate Desktop: a cross-platform desktop contact-sheet builder in Rust + GPUI 0.2.2.

## Commands

- `cargo run --release` — launch
- `cargo run --release -- path/to/file.slate` — launch with a project
- `cargo test --workspace` — core + app tests
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all`
- Release: push a `v*` tag; `.github/workflows/release.yml` builds and publishes
  macOS, Linux and Windows artifacts.
- `cargo run -p slate-core --example sample_sheet` — regenerate `target/sample`
  and the design-check PDFs

## Shape

```
crates/slate-core   GUI-free: model, import, batch, layout, pdf
crates/slate-app    GPUI shell: app state, text field, panels, theme
```

- One file per domain in core. Do not add GPUI dependencies to `slate-core`.
- `slate-app` owns all dialogs, async tasks, thumbnails and interaction state.
- The main editing window is single-window. Sheet preview and About are native
  GPUI child windows; keep them as read-only views over `ContactSheet` instead
  of drawing modal overlays.
- `panels.rs` owns the drag-resizable rail geometry. Keep the centre at
  `CENTER_MIN` and clamp the widths on every render.
- `menubar.rs` owns every menu. On macOS, build native menus with
  `cx.set_menus` and hide the in-window topbar; on other platforms, keep the
  in-window menus. Do not duplicate a command on the same platform. Sync the
  native menus when recent projects, the theme or the Drive account changes.
- `history.rs` snapshots project and selection before mutating actions. A
  successful file rename clears history; a save marks older snapshots dirty.
  Keep both invariants when adding commands.
- `slate-core/store.rs` is a local SQLite catalogue only; `.slate` files
  stay portable. Remember a project after open/save and keep the recent list
  bounded. `store::data_directory()` is the one application-data path.
- `drive.rs` owns Google OAuth, the refresh token, Drive listing and streaming
  transfers. `drive_ui.rs` owns the Drive browser, thumbnails, selected stack,
  setup notice and the import/export workflows. Keep every Drive command in the
  Drive menu. Drive list responses must explicitly request `size` and
  `thumbnailLink`; thumbnails are bounded and cached.

## Rules

- Sheet geometry must come from `layout::plan_sheet`; the preview and PDF must
  never compute their own cells.
- `Photo.rotation` is clockwise quarter turns. `Photo::aspect` and
  `oriented_size` account for it; PDF export applies the same turns. Never
  mutate the stored pixel width/height for rotation.
- New project fields need `#[serde(default)]` and `Project::normalize` coverage
  so existing project files keep opening. Custom paper stores its free-entry
  width and height on `SheetSettings`; read the final page size through
  `paper_mm()`, never `Paper::mm()` directly. `Photo.captured_at` is
  `YYYY-MM-DD HH:MM:SS`; bulk rename expands its date tokens from it.
- Async project replacement increments `generation`; every thumbnail and import
  task checks it before touching state.
- Keep background concurrency bounded: 3 import/thumbnail jobs.
- The library uses `uniform_list`; never build all photo cells. Schedule thumbs
  only for the rendered range and keep the cache at `THUMB_CACHE`. The 10,000
  photo path must stay virtualised. A left press toggles the cell and starts a
  marquee; dragging selects or deselects every cell in the rectangle. Right
  press opens the single-photo rename menu. Do not add a second click toggle.
- JPEG thumbnails and PDF images go through `jpeg-decoder`'s reduced IDCT. Do
  not decode a full-resolution JPEG just to make a preview. Call
  `memory::release_free_pages` after a project open or import drains.
- `Project.photos` is a `PhotoSet`: unique, private ids, O(1) `photo(id)`, and
  copy-on-write snapshots. Use `Photo::new` and `photo.id()`; never add a linear
  id lookup or replace the set with a bare `Vec`.
- `ViewState` owns visible indices, tag counts and scope ids. Call
  `project_changed`, `filter_changed` or `selection_changed` after edits; panels
  must read the cache instead of scanning the project.
- `QuarterTurns` and `PhotoSize` carry the rotation and pixel invariants. Do not
  reintroduce `% 4`, zero-width or zero-height checks.
- Text field callbacks must not read or update the same field from inside its
  own event handler (GPUI entity re-borrow panic). Pass values through the
  callback instead.
- Rename targets are checked against existing files and duplicate targets before
  enabling Apply rename. Never add a clobbering rename path.
- PDFs are flat, RGB, no layers. Images are capped at 300 DPI for their cell.
- HEIC/RAW decoding goes through macOS `sips`; keep the fallback path working on
  other platforms by returning an error that the UI can display.
- Google Drive uses a Desktop OAuth client with PKCE and the fixed loopback
  callback `http://127.0.0.1:53682/oauth2callback`. Google still requires a
  secret for this client at the token endpoint. Official builds use
  `token_broker`; local development may use a local `client_secret`. Never
  compile a secret or token into a binary, commit one, or log one.
- Use `google-drive` for OAuth and folder listing. Transfers stream through
  `reqwest`; never buffer a complete photo or PDF in memory. Avoid
  `FileOps::create_or_update` because its update branch drops the patched
  metadata. Drive downloads check `generation` before they touch the project.
- `vendor/xattr` is a patched 0.2.3 crate: libc 0.2.191 removed `ENOATTR` on
  Linux. Remove the patch when `zed-async-tar` stops pulling xattr 0.2.

- The macOS main-window title stays empty. Use the native app menu and native
  command menus; do not set a visible main-window title. The titlebar is
  transparent, so the sidebar, library and inspector backgrounds run to the
  window edge. Keep `theme::WINDOW_CHROME_TOP` clear for the traffic lights and
  use the sidebar strip as the drag region. Native child windows keep their
  standard titles.

## Style

High contrast, terminal-inspired: bundled Geist for names and actions, bundled
Geist Mono for data and shortcut hints, 1 px rules, square controls, SVG icons,
no rounded corners and no decorative gradients. Light and Dark are monochrome.
Palette themes use their accent only for selected badges, checkboxes, focused
fields and primary controls; do not add other colour uses. Theme choices live
under View only and iterate `Theme::ALL`. Use the `TEXT_*` tokens; body copy is
13 pt, labels 11 pt and metadata 10 pt. Use shadows to lift menus, photo frames,
paper preview and modals; do not add glass or blur.

Sentence case everywhere. Do not add uppercase copy, `to_uppercase()` calls or
CSS `text-transform: uppercase`. Acronyms such as PDF, RGB and DPI are fine.
Dense, not sparse. Do not add a wordmark, logo or tagline.

Do not repeat a label or action on screen. Each panel has a distinct job:
project and filters on the left, the library in the centre, the sheet on the
right. Do not add Project, Library or Sheet headings; the content is
self-evident. If two surfaces need the same concept, give one a different
purpose or use an icon. Use SVG icons for utility actions; reserve text for
primary actions. Put every global command in the menu bar; do not duplicate
export, save, import, selection or theme controls in the panels.

The first panel starts with the compact project name and path and carries the
selected count. On Windows and Linux the masthead carries only the in-window
menu bar; on macOS the native menu bar replaces it. Do not repeat the project
name or add action buttons there. Menu boxes start at an 8 px inset so their
labels sit on the same 16 px text gutter as the panels; hover fills the full
topbar height. Do not switch menus on hover.

Use `theme.rs` tokens, `widgets.rs` controls and `assets.rs` icons instead of
one-off styles. Load Geist in `main.rs`; never fall back to a system font.

## Automation

- `.github/workflows/ci.yml` runs fmt, clippy and tests on macOS, Linux and
  Windows.
- `.github/workflows/release.yml` packages a universal macOS `Slate.app`, a
  Linux tarball and a Windows zip on `v*` tags.
- Use only GitHub-owned actions, pinned to full commit SHAs. No third-party
  actions, no floating tags.
- Do not add a third-party release action; the release job uses `gh` with the
  workflow token.

## Tests

- Core: layout geometry/pagination/rotation, rename conflicts and apply,
  tag normalisation, project JSON roundtrip, PDF generation with real images,
  missing-file placeholders.
- App: pure helpers such as `slug`.
- Visual output: run `sample_sheet`, convert the PDF with
  `qlmanage -t -s 1400 -o target/sample target/sample/sample.pdf`, and inspect.
  Regenerate `docs/app-preview-light.jpg` and `docs/app-preview-dark.jpg` with
  the Chrome command in `docs/README.md`.
