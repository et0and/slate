# Repository guide

Slate Desktop: a cross-platform desktop contact-sheet builder in Rust + GPUI 0.2.2.

## Commands

- `cargo run --release` — launch
- `cargo run --release -- path/to/folder` — launch with a collection folder
- `cargo run --release -- path/to/file.slate` — open a legacy collection file
- `cargo test --workspace` — core + app tests
- `cargo lint` — Clippy, all targets, strict all/pedantic/nursery, warnings denied
- `cargo lint-fix` — apply Clippy's safe fixes
- `cargo fmt --all` — format
- `cargo fmt-check` — verify formatting without writing
- Release: merge to `main`; semantic-release reads Conventional Commits, updates
  the version and changelog, tags `v*`, creates the GitHub Release, and builds
  macOS, Linux and Windows artifacts from that tag.
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
  GPUI child windows. The photo preview is a native full-screen window. Both
  previews stay read-only views over `ContactSheet`. The photo preview opens from
  the grid eye button. The sheet builder is a right-rail mode opened by
  `File > Create contact sheet`; the right rail defaults to the selected photo
  inspector.
- `panels.rs` owns the drag-resizable rail geometry and the collapsed state.
  Keep the centre at `CENTER_MIN`, clamp the widths on every render, and use
  `left_edge`/`right_edge` for library/marquee geometry. A collapsed left pane
  disappears; its only control is the titlebar panel button next to the window
  controls. A collapsed right pane stays a narrow rail with one panel-icon
  expand button. Resetting panel widths expands both panes.
- `menubar.rs` owns every menu. On macOS, build native menus with
  `cx.set_menus` and hide the in-window topbar; on other platforms, keep the
  in-window menus. Do not duplicate a command on the same platform. Sync the
  native menus when recent folders, the theme or the Drive account changes.
- `history.rs` snapshots the collection and selection before mutating actions.
  `PhotoSet` snapshots share chunks and `Selection` is a roaring bitmap, so
  import/remove/sheet boundaries stay structural. A successful file rename
  clears history; a save marks older snapshots dirty. Keep both invariants when
  adding commands.
- `slate-core/store.rs` is a local SQLite catalogue and settings store. The
  filesystem is the collection hierarchy: a folder is a collection, a parent
  folder is a set, and `collection.slate` lives visibly beside the photos.
  Remember folders after open/save, persist the theme there, and keep the recent
  list bounded. `store::data_directory()` is the one application-data path.
- `drive.rs` owns Google OAuth, the refresh token, Drive listing and streaming
  transfers. `drive_ui.rs` owns the Drive browser, thumbnails, selected stack,
  setup notice and the import/export workflows. There is no Drive menu: Drive
  import, PDF export and unlink live in the File menu, and the Folders "+" menu
  offers Google Drive beside local folders. All of them go through
  `open_drive`, which links first and then continues. Imports
  mirror Drive's folder names under `drive::mount_directory()` and open the
  mirrored folder as an ordinary folder collection; the Folders section shows
  that mirror as the Google Drive tree. Downloads stream to `.part` files, reuse
  an existing same-size copy and never overwrite a different file. Drive
  list responses must explicitly request `size` and `thumbnailLink`;
  thumbnails are bounded and cached.

## Rules

- Sheet geometry must come from `layout::plan_page` / `plan_page_refs`; the
  preview and streaming PDF must never compute their own cells. Keep
  `plan_sheet` for tests and whole-plan diagnostics, not for export.
- `Photo.rotation` is clockwise quarter turns. `Photo::aspect` and
  `oriented_size` account for it; PDF export applies the same turns. Never
  mutate the stored pixel width/height for rotation. `Photo.rating` is 0–5;
  `Photo.label` is one of the fixed colour labels. Both use serde defaults so
  older collections open unchanged.
- New collection fields need `#[serde(default)]` and `Collection::normalize`
  coverage so existing collection files keep opening. `Project` is a
  compatibility alias for `Collection` while call sites migrate. Custom paper stores its free-entry
  width and height on `SheetSettings`; read the final page size through
  `paper_mm()`, never `Paper::mm()` directly. `Photo.captured_at` is
  `YYYY-MM-DD HH:MM:SS`; bulk rename expands its date tokens from it.
- Async collection replacement increments `generation`; every thumbnail and import
  task checks it before touching state.
- Keep background concurrency bounded: 3 import/thumbnail jobs.
- The library uses `uniform_list`; never build all photo cells. Schedule thumbs
  only for the rendered range and keep the cache at `THUMB_CACHE`. A left press
  on a photo fills the photo inspector and anchors a marquee; it never changes
  selection. The top-right checkbox is the only click toggle; do not add
  another. Dragging selects every cell in the rectangle, or deselects when the
  drag starts on a selected photo. The inspected photo has an ink frame;
  selected photos have a fill and a ticked checkbox. Double-click or the eye
  button opens the native full-screen preview without changing selection. Right
  press opens the per-photo menu: rename, show in inspector, reveal, copy path
  and remove.
- Empty selection means "all visible"; never materialise a 500k-photo selection
  just to open a collection. Selection is `Selection`, a roaring bitmap. Do not
  replace it with `HashSet<PhotoId>`; select-all must stay compact.
- `collection.slate` is the visible per-folder metadata file. Save photo paths
  inside the collection folder as relative paths so moving a folder keeps it
  portable. Opening a folder loads `collection.slate` or starts a new collection
  and imports immediate image files. The left folder tree mirrors the filesystem;
  do not add a virtual hierarchy to the catalog. The tree holds several roots
  (`library_roots`, persisted in the settings store): opening a folder outside
  every root adds its parent set, and a root's hover ✕ only removes it from the
  tree.
- Import probes metadata only. Generate a thumbnail when a cell becomes visible;
  do not decode one preview per imported file just to evict it from the cache.
  Append imported photos to `ViewState` incrementally; full view reconciles
  happen once at import completion, not every 100 ms.
- Photo edits use `history` field deltas (tags, rotation, capture date, rating,
  label, adjustments). Do not
  add a full-collection snapshot for a single-photo edit. Whole-collection snapshots
  are for sheet/saved-sheet, import and removal boundaries, and are cheap
  because they share photo chunks and the selection bitmap.
- Save collections, plan renames and apply rename filesystem work on the background
  executor. Never block the UI with a 500k-photo JSON write or file operation.
- PDF export writes one page at a time. Do not create a `SheetPlan` or hold all
  page pixels for a 500k-photo export; the output is already tens of thousands
  of pages.
- Telemetry is opt-in through `AXIOM_TOKEN`. Use `tracing` spans in core and
  app code, never log tokens, file paths or image data, and keep per-photo spans
  at trace level. The Axiom exporter flushes on process exit. See
  `docs/telemetry.md`.
- JPEG export embeds the original DCT stream. All EXIF orientations, including
  mirrors, are represented by the PDF image matrix; CMYK uses `/DeviceCMYK`
  and the Adobe decode inversion. Do not reintroduce decode/resize/encode for
  unadjusted JPEGs; only a photo with non-neutral adjustments is decoded,
  adjusted and re-encoded.
- Adjustments (`slate_core::adjust`) are non-destructive: one sparse
  `AdjustmentMap` on the collection, behind an `Arc` so snapshots share it,
  never a field on `Photo` (it must stay 80 bytes). Neutral adjustments are not
  stored. Thumbnails, the inspector, the full-screen preview, PDF export and
  conversion all apply them; originals are never written. `Slider` owns each
  control's range, step, neutral value and label, so the UI, the normaliser and
  the pipeline cannot disagree. The inspector edits the inspected photo: a drag
  updates live from the cached decode (`inspector_base`, one render in flight)
  and records one undo step on release. Undo and bulk copies share
  `Arc<Adjustments>` values. Colour lookup stores only a `LutId` per photo; the
  collection maps ids to `.cube` paths, and `adjust::load_lut_file` parses into
  a process-wide cache that previews and exports read. A missing LUT skips that
  stage rather than failing the photo.
- Hue-picking sliders paint the hue spectrum on their track, and the gradient
  map shows its gradient. They are the only colour besides the theme accent
  and the label colours.
- Non-interlaced 8-bit RGB/gray PNGs embed their IDAT stream directly with
  `/FlateDecode` and predictor 15. Alpha, palette, 16-bit and interlaced PNGs
  use the fallback path.
- HEIC/RAW use in-process ImageIO thumbnails on macOS; `sips` remains a
  fallback only. Page images are prepared in parallel with Rayon, but written
  sequentially in placement order.
- Use the PDF xref stream (`/W [1 8 2]`) with 64-bit offsets. A classic xref
  table is capped at ten decimal offset digits and cannot describe a 500k
  export.
- JPEG thumbnails and fallback PDF images go through `jpeg-decoder`'s reduced
  IDCT. The direct DCT export path bypasses decode entirely. Do not decode a
  full-resolution JPEG just to make a preview. Call
  `memory::release_free_pages` after a collection open or import drains.
- `Collection.photos` is a chunked copy-on-write `PhotoSet` with a persistent
  position index and O(1) `photo(id)`. Mutating one photo clones at most one
  1024-photo chunk. Use `Photo::new` and `photo.id()`; never add a linear id
  lookup or replace the set with a bare `Vec`. Keep `Photo` at 80 bytes:
  path/tags/capture date are shared or boxed, and a size regression test guards
  it.
- `ViewState` owns sorted visible indices, tag/rating/label counts and scope ids.
  Call `project_changed`, `filter_changed` or `selection_changed` after edits;
  panels must read the cache instead of scanning the collection. Sorting is applied
  after filtering; missing capture dates stay last in both directions.
- `QuarterTurns` and `PhotoSize` carry the rotation and pixel invariants. Do not
  reintroduce `% 4`, zero-width or zero-height checks.
- Text field callbacks must not read or update the same field from inside its
  own event handler (GPUI entity re-borrow panic). Pass values through the
  callback instead, and `cx.defer` any update to that field, such as clearing it
  after submit.
- In-window dialogs use `widgets::modal`, and menus occlude their dismiss layer.
  Without `occlude()` a press inside the panel also reaches the backdrop and
  closes it before the control handles its click.
- A child window cannot update itself from inside its own event dispatch.
  Defer `remove_window` when the request starts in that window.
- Escape closes the topmost transient surface, one per press. In the search
  field it clears the query, then returns focus to the library.
- The photo inspector reads EXIF on demand in a background task and caches the
  result; never decode an image just for metadata. Editing the capture date
  updates `Photo.captured_at` in `collection.slate` only. Do not claim to write
  EXIF to the original file without a separate, backed-up file-writing path.
- Rename targets are checked against existing files and duplicate targets before
  enabling Apply rename. Never add a clobbering rename path.
- PDFs are flat, RGB, no layers. Images are capped at 300 DPI for their cell.
- Colour labels are the only intentional use of colour outside the active
  theme accent. Keep the five fixed label colours recognisable in every theme.
- HEIC/RAW import decoding goes through macOS `sips`; PDF export uses ImageIO.
  Keep the non-macOS fallback returning an error that the UI can display.
- PSD import uses the pure-Rust `psd` crate and flattens the composite. DDS,
  EXR, HDR, ICO, PNM, QOI and TGA use the `image` codecs. Batch conversion is
  JPEG/PNG/WebP/TIFF only and must not overwrite an existing target.
- ImageMagick is not a dependency. A MagickWand system library cannot be
  assumed by the release pipeline, so keep conversion on the bundled codecs.
- Video import (`slate_core::video`, `video_ui.rs`) uses `ffmpeg-next`. A video
  is divided into N equal parts and the frame at the middle of each is decoded
  (keyframe seek, then decode forward to the moment). Frames are JPEGs in a
  `<stem> frames` folder beside the video, named `<stem>-NNN-<timecode>.jpg`,
  never overwriting; the folder then opens as a folder collection. The display
  matrix rotates phone video upright. One background job per import reads an
  `AtomicBool` to cancel and counts frames in an `AtomicUsize`; the dialog only
  polls them. A cancelled or failed job removes every frame it wrote.
- Every file import opens through `app::pick_files` with a `FileKinds`
  (photos, videos, LUTs): GPUI's prompt cannot filter by type, so these use
  `rfd`'s native panel, where unsupported files are greyed out (macOS) or
  hidden. Folder pickers stay on GPUI's prompt. A drop that contains only
  unsupported files says so in the status bar rather than doing nothing.
- Development links the system FFmpeg through pkg-config. Releases must not
  assume one: macOS and Linux build with `--features bundled-ffmpeg`, which
  compiles FFmpeg from source, statically and with `build-portable` so no
  `-march=native` code ships; Windows links vcpkg's
  `x64-windows-static-md` FFmpeg with `--features static-ffmpeg`. The test clips
  in `crates/slate-core/tests/fixtures` come from FFmpeg's `testsrc`.
- Google Drive uses a Desktop OAuth client with PKCE and the fixed loopback
  callback `http://127.0.0.1:53682/oauth2callback`. Google still requires a
  secret for this client at the token endpoint. Official builds use
  `token_broker`; local development may use a local `client_secret`. Never
  compile a secret or token into a binary, commit one, or log one.
- Use `google-drive` for OAuth and folder listing. Transfers stream through
  `reqwest`; never buffer a complete photo or PDF in memory. Avoid
  `FileOps::create_or_update` because its update branch drops the patched
  metadata. Drive downloads check `generation` before they touch the collection.
- `vendor/xattr` is a patched 0.2.3 crate: libc 0.2.191 removed `ENOATTR` on
  Linux. Remove the patch when `zed-async-tar` stops pulling xattr 0.2.
- `vendor/ffmpeg-sys-next` is a patched 9.0.1 crate: its cross build passes
  Rust's `macos` to FFmpeg's `configure`, which only knows `darwin`, so the
  universal app's x86_64 half fails on Apple silicon. Remove the patch when an
  upstream release maps `macos` to `darwin`.

- The macOS main-window title stays empty. Use the native app menu and native
  command menus; do not set a visible main-window title. The titlebar is
  transparent, so the sidebar, library and inspector backgrounds run to the
  window edge. `chrome.rs` owns the `WINDOW_CHROME_TOP` strip, the left panel
  collapse button and the drag region. Double-click calls the native titlebar
  action so the window zooms. Native child windows keep their standard titles.

## Anti-slop

Adapted from wwwtom's anti-slop rules, expressed for Rust.

The strict Clippy profile enables `all`, `pedantic` and `nursery`. The allowlist
in `Cargo.toml` is limited to framework signatures, bounded pixel/PDF casts and
readability rules where a mechanical fix would make the code worse. Do not add
broad allowances; use `#[expect]` with a reason for a single justified case.

- Make the smallest correct change. Follow local patterns and improve the file
  you are already in.
- One function unless the logic is genuinely reusable. Keep the reason for a
  helper obvious from its name.
- Single-use converters, wrappers and adapters are slop unless they enforce a
  contract. Inline them.
- Parse unknown input once at the edge into typed structs and enums. Do not
  pass `serde_json::Value`, untyped maps or strings through the app.
- Do not widen a typed value into a string, integer or dictionary and cast it
  back later. Keep the original type.
- Use named structs and enums instead of boolean flags, tuple bags or opaque
  parameter groups. Use enums to make invalid states hard to represent.
- No runtime shape or type probing when a variant can represent the state.
- Do not chain `as` casts, `unwrap()` or `expect()` to launder a value. Use
  `?`, `match` or `let else` at boundary contracts. Tests may unwrap.
- No speculative compatibility paths, deprecated aliases or dead fallback
  variants. Update every caller and delete the old path in the same change.
- Names describe domain behaviour, not type shape. Avoid `FooData`,
  `FooHelper` and `FooUtil`.
- Do not duplicate logic. Use the existing local helper. Comments explain why,
  invariants or platform constraints; they do not restate code.
- Do not add `dbg!`, `todo!` or `unimplemented!`. Handle the error or return a
  concrete failure.

## Style

High contrast, terminal-inspired: the platform UI font (`.SystemUIFont`: San
Francisco, Segoe UI, the desktop sans) for names and actions, bundled Geist
Mono for data and shortcut hints, 1 px rules, square controls, SVG icons,
no rounded corners and no decorative gradients. Light and Dark are monochrome.
Palette themes use their accent only for selected badges, checkboxes, focused
fields and primary controls; do not add other colour uses. Theme choices live
under View only and iterate `Theme::ALL`. Use the four `TEXT_*` tokens and never
offset one by a fraction: display 20 pt for the one heading of an in-window
dialog or empty state (`widgets::heading`), body 13 pt, labels 11 pt and
metadata 10 pt. A native child window's title bar is its heading: do not repeat
it, or its close button, inside the window. Shortcut hints use `widgets::shortcut_hint`
at body size so the modifier glyphs stay legible. Weight encodes the role, never the
state: headings semibold, controls medium, content and menu rows regular.
Selection, focus and disabled states change colour or fill, not weight. Use
shadows to lift menus, photo frames, paper preview and modals; do not add glass
or blur.

Do not add instructional or hint copy: no "Click…", "Select…", "Try…",
"Add a folder with +" or "Save a layout to reuse it". An empty area stays empty
or lists real commands; a control's label says what it does. Keep only facts the
user cannot see otherwise, such as "This cannot be undone".

Do not end menu items or button labels with an ellipsis. Ellipses belong only
to progress text such as "Scanning…".

Sentence case everywhere. Do not add uppercase copy, `to_uppercase()` calls or
CSS `text-transform: uppercase`. Acronyms such as PDF, RGB and DPI are fine.
Dense, not sparse. Do not add a tagline. The pixel SLATE wordmark
(`widgets::pixel_wordmark`) appears only at the foot of the empty library, in
the theme's surface colour and sized to the pane width, and beside the block
mark in the About window.

Do not repeat a label or action on screen. Each panel has a distinct job:
collection, folders, review and filters on the left, the library in the centre, the photo
inspector or sheet builder on the right. Do not add Collection or Library headings;
the content is self-evident. The sheet builder may use one compact mode heading.
If two surfaces need the same concept, give one a different purpose or use an
icon. Use SVG icons for utility actions; reserve text for primary actions. Put
every global command in the menu bar; do not duplicate export, save, import,
selection or theme controls in the panels.

The first panel starts with the folder tree, then Search. Do not repeat the collection name or path in
the panels. Keep each left-rail section collapsible with a chevron and persist
the selected theme across launches. Use the panel icons, not chevrons, for pane
collapse and expand. On Windows and Linux the masthead carries only the
in-window menu bar; on macOS the native menu bar replaces it. Menu boxes start
at an 8 px inset so their labels sit on the same 16 px text gutter as the
panels; hover fills the full topbar height. Do not switch menus on hover.

Use `theme.rs` tokens, `widgets.rs` controls and `assets.rs` icons instead of
one-off styles. Load Geist Mono through `assets::install_fonts`; do not bundle a
second interface font over the platform one.

## Automation

- `.github/workflows/ci.yml` runs fmt, clippy and tests on macOS, Linux and
  Windows, plus a release-mode 500k synthetic stress gate on Linux.
- `.github/workflows/release.yml` runs semantic-release on `main`, then builds
  from the release tag and uploads three direct downloads, never archives:
  `Slate-macos-universal.dmg` (universal `Slate.app`), `Slate-windows-x86_64.exe`
  and `Slate-linux-x86_64.AppImage`. The AppImage is the pinned, checksum-verified
  AppImage runtime followed by a squashfs of the AppDir; bump the runtime URL and
  SHA-256 together. See `docs/release.md` for the versioning and notarization
  rules.
- Conventional commit subjects are release input. The first release is the
  bootstrapped `v0.0.1` tag; after that `feat` is minor, `fix` and `perf` are
  patch, breaking changes are major, and `chore`/docs do not release. Squash
  titles must follow the same format.
- Use only GitHub-owned actions, pinned to full commit SHAs. No third-party
  actions, no floating tags.
- Do not add a third-party release action; semantic-release is installed through
  npm and the upload job uses `gh` with the workflow token.
- macOS release builds use a Developer ID certificate, hardened runtime and
  `notarytool` when the six secrets in `docs/release.md` are configured. Do not
  commit certificates, passwords or notarization credentials. Without those
  secrets the workflow produces an ad-hoc signed build and warns.

## Tests

- Core: layout geometry/pagination/rotation, rename conflicts and apply,
  tag normalisation, collection JSON roundtrip, PDF generation with real images,
  missing-file placeholders.
- App: GPUI `TestAppContext` flows for rating, labels, sorting, photo
  inspection, sheet builder, conversion and the native preview window, plus pure
  helpers such as `slug`.
- Stress: `cargo run --release -p slate-core --example stress -- --photos 500000 --gate`
  enforces file size, load, planning, rename and base-RSS limits on Linux CI.
- Visual output: run `sample_sheet`, convert the PDF with
  `qlmanage -t -s 1400 -o target/sample target/sample/sample.pdf`, and inspect.
  Regenerate `docs/app-preview-light.jpg` and `docs/app-preview-dark.jpg` with
  the Chrome command in `docs/README.md`.
