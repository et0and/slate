# Slate Desktop

A desktop contact-sheet builder for photographers. Built in Rust with
[GPUI](https://gpui.rs), the GPU UI framework behind Zed.

Import a folder, tag and batch-rename, lay out a proof sheet, export a flat PDF,
and save the project or a sheet preset to export again later.

## Interface

![Slate Desktop in light mode](docs/app-preview-light.jpg)

![Slate Desktop in dark mode](docs/app-preview-dark.jpg)

The workspace in light and dark modes: project and filters, the library grid,
batch rename, live page preview and sheet controls. The first panel starts with
the project name, file path and selected count; the topbar is only the menu bar.
All global commands live there, so the panels do not repeat action buttons.

![View menu in One Dark](docs/menu-preview.jpg)

*The View menu with Light, Dark, One Dark, Monokai and Catppuccin. File, Edit,
History and Help sit beside it; all global commands are in the menu bar.*

## Sheet output

![Example exported contact sheet](docs/example-sheet.jpg)

*Real PDF output, using the Unsplash sample photographs in `assets/sample`.*

![A4 landscape sheet with rotate to fit](docs/example-rotated.jpg)

*A4 landscape, 4 × 3, rotate-to-fit enabled.*

## What it does

- **Bulk import** files and folders, recursively. Drag and drop works too.
- **Batch operations** on the current selection: pattern rename on disk, add and
  remove tags, clear tags, remove photos from the project.
- **Undo and redo** for project edits and sheet settings, up to 40 steps.
  Renames clear the history because disk changes cannot be reversed safely.
- **Desktop menu bar** with File, Edit, View, History and Help menus: open,
  save and save as, import, export, save sheet, undo and redo, selection,
  themes, reset panel widths, session history and about. There are no duplicate
  toolbar buttons for these commands.
- **Palettes**: Light and Dark are the monochrome defaults and follow the system
  appearance at launch. One Dark, Monokai and Catppuccin are available under
  View; their accent colours appear only in selected states and primary
  controls.
- **Filter** by tag or filename and switch between all, tagged and untagged
  images.
- **Per-photo rotation** from the control on each thumbnail, left of the
  selection tick. Quarter turns are stored in the project and applied to the
  preview and PDF.
- **Sheet editor** with a live page preview: paper, orientation, columns, rows,
  margins, gutters, filenames, index numbers, rotate-to-fit, header and footer.
- **Resizable panels**: drag the vertical dividers to widen the library or sheet
  rail. The centre adapts and the grid reflows.
- **Flat PDF export** with one image per cell, at up to 300 DPI for the cell
  size. No layers, no transparency.
- **Projects and saved sheets** stored as plain JSON (`.contactsheet`), so a
  sheet can be reopened and exported later.
- **Local catalogue** in SQLite for recent projects: name, path, photo count and
  last update time. The catalogue never replaces the portable project file.
- **One-window, keyboard-first workflow**: open, save, save as, export and undo
  shortcuts, plus import, select-all and dismiss. macOS uses `⌘`; Windows and
  Linux use `Ctrl`. Menus show the active platform's hints.

## Run

Runs on macOS 11+, Windows 10/11, and Linux with X11 or Wayland. Builds use Rust
1.99.1. On macOS, full Xcode is not required: GPUI's Metal shaders are compiled
at runtime (the `runtime_shaders` feature), so the Command Line Tools are
enough. HEIC/RAW decoding still uses macOS ImageIO; other platforms import those
files as placeholders.

```sh
cargo run --release
```

Open a project directly:

```sh
cargo run --release -- ~/Pictures/street-walk.contactsheet
```

Generate a sample project and two exported sheets for a design check:

```sh
cargo run -p slate-core --example sample_sheet
# outputs are in target/sample
```

## Releases

CI builds and tests macOS, Linux and Windows. A `v*` tag publishes:

| Platform | Artifact |
| --- | --- |
| macOS (universal) | `slate-desktop-macos-universal.zip` containing `Slate.app` |
| Linux x86_64 | `slate-desktop-linux-x86_64.tar.gz` |
| Windows x86_64 | `slate-desktop-windows-x86_64.zip` |

The macOS app is ad-hoc signed, not notarized. The release and CI workflows use
only first-party GitHub Actions, pinned to full commit SHAs; there are no
third-party actions.

Tag a release from `main`:

```sh
git tag v0.1.0
git push origin v0.1.0
```

## Workflow

1. **Import** from **File > Import files** / **Import folder**, or by dropping
   photos into the window. Imported photos are selected automatically and
   previews build in the background.
2. **Filter and select.** With no explicit selection, collated sheets use every
   visible photo. **Edit > Select all visible**, **Deselect all** and
   **Invert selection** change the explicit selection.
3. **Tag and rename.** Batch operations apply to the selection, or to every
   visible photo when nothing is selected. Review shows a full before/after list
   and refuses to apply when a target already exists.
4. **Build the sheet** in the right rail. The preview updates immediately,
   including rotate-to-fit.
5. **Save a sheet** to keep that layout and photo set in the project, or export
   the current sheet directly.
6. **Save the project.** A `.contactsheet` file contains photo paths, tags,
   sheet settings and saved sheets.

## Sheet options

| Option | Values |
| --- | --- |
| Paper | A4, A3, Letter, Tabloid |
| Orientation | Portrait, Landscape |
| Grid | 1–12 columns × 1–12 rows |
| Margin | 2–40 mm, in 2 mm steps |
| Gutter | 0–20 mm, in 1 mm steps |
| Filenames | On / off |
| Index numbers | On / off |
| Rotate to fit | On / off (90° only when it makes the image larger in the cell) |
| Header / footer | On / off |
| Title | Custom, defaults to the project name |

## File support

Standard photo formats decode through the `image` crate: JPEG, PNG, TIFF,
WebP, GIF and BMP.

On macOS, HEIC/HEIF and camera RAW (DNG, CR2, CR3, NEF, ARW, RAF, RW2, ORF, PEF,
SRW, 3FR) are decoded through ImageIO via `sips`, including embedded previews
and orientation.

A file that cannot be decoded is still imported. It gets a placeholder icon in
the grid and prints as a “Missing” placeholder, so a batch is never silently
dropped.

## Local metadata

Projects stay as portable `.contactsheet` documents. The app also keeps a local
SQLite catalogue of the projects it has opened or saved: name, path, photo count
and last update. **File** lists those recent projects. The database lives in the
platform data directory:

- macOS: `~/Library/Application Support/slate-desktop/metadata.sqlite3`
- Windows: `%APPDATA%\slate-desktop\metadata.sqlite3`
- Linux: `$XDG_DATA_HOME/slate-desktop/metadata.sqlite3` or
  `~/.local/share/slate-desktop/metadata.sqlite3`

Set `SLATE_DATA_DIR` to override the directory for development or tests.

## Data and safety notes

- Renames are real filesystem renames, conflict-checked before applying. There
  is no undo for the disk operation; the project history is cleared after a
  successful rename.
- Tags are stored in the project file, not written into EXIF/IPTC metadata.
- “Remove from project” never deletes files from disk.
- Project files store absolute photo paths.
- PDF labels use the built-in Helvetica and Courier fonts, so non-ASCII
  filenames are replaced with `?` in the PDF. The project keeps the real name.
- Preview thumbnails live in memory (320 px longest edge). PDF export decodes
  the originals again and caps each image at 300 DPI for its cell, which also
  keeps large exports reasonable.

## Design

The interface is monochrome and high contrast, with a terminal-inspired,
dense table language: Geist for names and actions, Geist Mono for data and
shortcut hints, hairline rules, square controls, SVG icons and no rounded
corners. Shadows lift the topbar, menus, photo frames, paper preview and
modals. Labels use sentence case throughout; no copy is forced to uppercase and
no label or action is repeated on screen. The menubar exposes the global
command set; the left, centre and right rails have no panel headings and no
duplicate action buttons. Selected counts live with the project. The library
and sheet rails are drag-resizable. Light and Dark are monochrome; One Dark,
Monokai and Catppuccin add their palette accent only to selected states and
primary controls. The PDF stays paper-white with a technical caption strip,
hairline grid and compact header/footer.

## Architecture

```
crates/slate-core/     GUI-free engine
  model.rs             project, photos, sheet settings, saved sheets
  import.rs            discovery, EXIF orientation, thumbnails, sips fallback
  batch.rs             rename planning/apply, tag normalisation
  layout.rs            page geometry and pagination
  pdf.rs               flat PDF renderer
  store.rs             local SQLite project catalogue
crates/slate-app/      GPUI desktop shell
  app.rs               state, async workflows, dialogs, selection
  text_field.rs        single-line text field (IME, selection, clipboard)
  library.rs           image grid and empty states
  sidebar.rs           filters, tags, batch rename, log
  inspector.rs         page preview and sheet settings
  overlay.rs           rename confirmation
  chrome.rs            menubar container and status bar
  menubar.rs           File, Edit, View, History and Help menus
  history.rs           bounded undo and redo snapshots
  panels.rs            resizable rail geometry
  widgets.rs           shared controls and type styles
  assets.rs            bundled Geist fonts and SVG icons
  theme.rs             colours and type scale
```

The core crate has no GUI dependency, so layout, batching and PDF export are
unit-tested headlessly.

## Verify

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Limits

- GPUI is pinned to `0.2.2`, the latest crates.io release.
- Undo is bounded to 40 project snapshots. A file rename clears it.
- Theme choice applies to the current session.
- No multi-window support or EXIF writing yet.
- HEIC/RAW support is macOS-only.
- macOS releases are ad-hoc signed, not notarized.
- Saved sheets reference photo IDs; missing files export as placeholders.

## Bundled assets

- Geist and Geist Mono are bundled under the SIL Open Font License. See
  `assets/OFL.txt`.
- Sample photographs in `assets/sample` are from Unsplash under the Unsplash
  License. See `assets/sample/ATTRIBUTION.md`.

## License

MIT
