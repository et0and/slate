# Slate Desktop

A desktop contact-sheet builder for photographers. Built in Rust with
[GPUI](https://gpui.rs), the GPU UI framework behind Zed.

Import a folder, tag and batch-rename, lay out a proof sheet, export a flat PDF,
and save the project or a sheet preset to export again later.

## Interface

![Slate Desktop in light mode](docs/app-preview-light.jpg)

![Slate Desktop in dark mode](docs/app-preview-dark.jpg)

The workspace in light and dark modes: library grid, tag filters, batch rename,
live page preview and sheet controls.

## Sheet output

![Example exported contact sheet](docs/example-sheet.jpg)

*Real PDF output, using synthetic frames from the included `sample_sheet`
example.*

![A4 landscape sheet with rotate to fit](docs/example-rotated.jpg)

*A4 landscape, 4 × 3, rotate-to-fit enabled.*

## What it does

- **Bulk import** files and folders, recursively. Drag and drop works too.
- **Batch operations** on the current selection: pattern rename on disk, add and
  remove tags, clear tags, remove photos from the project.
- **Filter** by tag or filename and switch between all, tagged and untagged
  images.
- **Sheet editor** with a live page preview: paper, orientation, columns, rows,
  margins, gutters, filenames, index numbers, rotate-to-fit, header and footer.
- **Flat PDF export** with one image per cell, at up to 300 DPI for the cell
  size. No layers, no transparency.
- **Projects and saved sheets** stored as plain JSON (`.contactsheet`), so a
  sheet can be reopened and exported later.
- **Light and dark modes**, monochrome and high contrast. The app follows the
  system appearance at launch and switches from the masthead.
- **One-window, keyboard-first workflow**: `⌘O` open, `⌘S` save, `⌘E` export,
  `⌘I` import files, `⇧⌘I` import folder, `Esc` dismiss.

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
inspect the outputs in target/sample
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

1. **Import** with **Import files**, **Import folder**, or by dropping photos
   into the window. Imported photos are selected automatically and previews
   build in the background.
2. **Filter and select.** With no explicit selection, collated sheets use every
   visible photo. **All**, **None** and **Invert** change the explicit
   selection.
3. **Tag and rename.** Batch operations apply to the selection, or to every
   visible photo when nothing is selected. Rename shows a full before/after list
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

A file that cannot be decoded is still imported. It appears as a
“No preview” cell and prints as a “Missing” placeholder, so a batch is never
silently dropped.

## Data and safety notes

- Renames are real filesystem renames, conflict-checked before applying. There
  is no undo.
- Tags are stored in the project file, not written into EXIF/IPTC metadata.
- “Remove from project” never deletes files from disk.
- Project files store absolute photo paths.
- PDF labels use the built-in Helvetica and Courier fonts, so non-ASCII
  filenames are replaced with `?` in the PDF. The project keeps the real name.
- Preview thumbnails live in memory (320 px longest edge). PDF export decodes
  the originals again and caps each image at 300 DPI for its cell, which also
  keeps large exports reasonable.

## Design

The interface is monochrome and high contrast: true black and white, neutral
greys, hairline rules, no rounded corners and no decorative gradients. Restrained
shadows lift only the paper preview and the confirmation modal. Labels use
sentence case throughout; no copy is forced to uppercase.
Metadata is monospaced and dense, while sheet titles and photo names carry the
hierarchy. Light and dark modes follow the system appearance at launch and can
be switched from the masthead. The PDF stays paper-white with a technical
caption strip, hairline grid and compact header/footer.

## Architecture

```
crates/slate-core/     GUI-free engine
  model.rs             project, photos, sheet settings, saved sheets
  import.rs            discovery, EXIF orientation, thumbnails, sips fallback
  batch.rs             rename planning/apply, tag normalisation
  layout.rs            page geometry and pagination
  pdf.rs               flat PDF renderer
crates/slate-app/      GPUI desktop shell
  app.rs               state, async workflows, dialogs, selection
  text_field.rs        single-line text field (IME, selection, clipboard)
  library.rs           image grid and empty states
  sidebar.rs           filters, tags, batch rename, log
  inspector.rs         page preview and sheet settings
  overlay.rs           rename confirmation
  chrome.rs            masthead and status bar
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
- No undo history, multi-window support or EXIF writing yet.
- HEIC/RAW support is macOS-only.
- macOS releases are ad-hoc signed, not notarized.
- Saved sheets reference photo IDs; missing files export as placeholders.

## License

MIT
