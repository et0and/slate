<img width="2865" height="2037" alt="desktop" src="https://github.com/user-attachments/assets/e4d7a212-3088-407f-9689-b6fcb4bb88b0" />

<p align="center">
  <a href="https://tom.so/work/slate">Website</a>
  ·
  <a href="https://github.com/et0and/slate/releases">Releases</a>
  ·
  <a href="https://github.com/et0and/slate/issues">Issues</a>
</p>

Slate is a desktop photography workflow tool. Open a folder of photos and take
it from first look to finished output: cull, rate, label and tag, rename in
batches, convert copies, and print contact sheets. Slate never imports your
library into a database.

Every folder is a collection. Its ratings, labels, tags, rotations, capture-date
edits and saved sheets live in a small `collection.slate` file beside the
photos. Move, back up or share the folder and it opens just as you left it. The
folder tree on the left mirrors your disk, and a folder's parent is its set.

Use Slate for the first pass through a shoot, client proofs, portfolio edits and
archive indexes. 

### Open

- Open a folder, drop files or folders onto the window, or press **+** in the
  Folders pane to add a local folder or a Google Drive folder. The tree keeps
  every folder you add, side by side.
- Click any folder in the tree to switch collections. Slate saves the current
  one first.
- Reads JPEG, PNG, TIFF, WebP, GIF, BMP, PSD, DDS, EXR, HDR, ICO, PNM, QOI and
  TGA on every platform, plus HEIC and camera RAW (DNG, CR2, CR3, NEF, ARW, RAF,
  RW2, ORF, PEF, SRW, 3FR) on macOS.

### Review

- Browse a virtualised grid that loads only the thumbnails on screen. Click a
  photo to inspect it; tick its checkbox to select it, or drag across several.
- Open any frame full screen by double-clicking it or with its eye button. Arrow keys step through the
  sorted library; scroll to zoom up to 8× and drag to pan.
- Select a photo to fill the inspector with its dimensions, size, folder, tags
  and EXIF. Correct a wrong capture date there; the fix is stored in the
  collection and the original file is left alone.
- Rotate frames from the thumbnail or the inspector. Rotation is a setting, so
  the pixels on disk never change.

### Cull and organise

- Give photos 0–5 stars, one of five colour labels, and any number of tags.
  Each acts on the selection, or on every visible photo when nothing is
  selected.
- Search names and tags with <kbd>⌘K</kbd>. Filter to untagged, rated, unrated,
  a tag or a label, and sort by name, capture date, size, type, rating or label.
- Undo and redo every edit. Edits are recorded per field, so undo stays instant
  in a 500,000-photo collection.

### Adjust

- Tone: exposure, brightness, contrast, shadows and highlights.
- Colour: temperature, tint, vibrance, saturation, hue, black & white and
  desaturate.
- Effects: invert, posterize and threshold.
- Adjustments are non-destructive. They are stored in `collection.slate`, show
  everywhere the photo does, and are baked into PDFs and converted copies. Copy
  one photo's adjustments to the whole selection, or reset them, in one step.

### Rename and convert

- Batch rename with a prefix, suffix, counter and padding, plus `{date}`,
  `{time}`, `{datetime}` and `{year}` tokens filled from each capture date.
  Review every change before it runs: a name that clashes with an existing file
  blocks the rename, and nothing is overwritten. Right-click a photo to rename it
  on its own.
- Convert copies to JPEG, PNG, WebP or TIFF in a folder you choose. Originals
  are untouched and existing files are never replaced.

### Print contact sheets

- **File > Create contact sheet** opens the sheet builder: A4, A3, Letter,
  Tabloid or a custom paper size, orientation, grid, margin, gutter, captions,
  index numbers, rotate-to-fit, header and footer, all with a live page preview.
- Save a layout as a named sheet, then load or re-export it later.
- Export a flat RGB PDF at up to 300 DPI per cell. Pages stream to disk one at a
  time and JPEGs are embedded without recompression, so a thousand-page proof
  doesn't need a thousand pages of memory.
- On modest machines, Photoshop's contact-sheet automation is
  [slow and unreliable](https://www.reddit.com/r/photoshop/comments/vpwwn7/attempting_to_create_a_contact_sheet_that_uses/).
  Slate plans an 834-page sheet in 3 ms.

### Google Drive

Link a Drive account to browse folders, import a selection or a whole folder,
and export a finished PDF straight to a Drive folder. Imports are mirrored
locally with Drive's own folder structure and open as an ordinary folder
collection; the Folders pane shows the mirror under Google Drive, so it browses
exactly like a local disk. Importing a folder again only downloads what's new.
Setup takes one Desktop OAuth client: see
[Google Drive setup](docs/google-drive.md).

## Your files stay yours

Slate reads photos in place and only touches them when you ask it to rename
them. Adjustments never write pixels back to the original.
`collection.slate` holds metadata, not image data, and stores paths
relative to the folder so the collection stays portable. Conversion and export
always write new files.

## Keyboard

| Action | macOS | Windows and Linux |
| --- | --- | --- |
| Search | <kbd>⌘K</kbd> or <kbd>⌘F</kbd> | <kbd>Ctrl+K</kbd> or <kbd>Ctrl+F</kbd> |
| Open folder | <kbd>⌘O</kbd> | <kbd>Ctrl+O</kbd> |
| New collection | <kbd>⌘N</kbd> | <kbd>Ctrl+N</kbd> |
| Save collection / save as | <kbd>⌘S</kbd> / <kbd>⇧⌘S</kbd> | <kbd>Ctrl+S</kbd> / <kbd>Ctrl+Shift+S</kbd> |
| Import files / import folder | <kbd>⌘I</kbd> / <kbd>⇧⌘I</kbd> | <kbd>Ctrl+I</kbd> / <kbd>Ctrl+Shift+I</kbd> |
| Select all visible / deselect | <kbd>⌘A</kbd> / <kbd>⇧⌘D</kbd> | <kbd>Ctrl+A</kbd> / <kbd>Ctrl+Shift+D</kbd> |
| Undo / redo | <kbd>⌘Z</kbd> / <kbd>⇧⌘Z</kbd> | <kbd>Ctrl+Z</kbd> / <kbd>Ctrl+Shift+Z</kbd> |
| Rename | <kbd>⌘R</kbd> | <kbd>Ctrl+R</kbd> |
| Convert selected | <kbd>⇧⌘E</kbd> | <kbd>Ctrl+Shift+E</kbd> |
| Create contact sheet | <kbd>⇧⌘C</kbd> | <kbd>Ctrl+Shift+C</kbd> |
| Sheet preview | <kbd>⇧⌘P</kbd> | <kbd>Ctrl+Shift+P</kbd> |
| Export PDF | <kbd>⌘E</kbd> | <kbd>Ctrl+E</kbd> |
| Toggle left / right pane | <kbd>⌘B</kbd> / <kbd>⌥⌘B</kbd> | <kbd>Ctrl+B</kbd> / <kbd>Ctrl+Alt+B</kbd> |
| Previous / next in a preview | <kbd>←</kbd> / <kbd>→</kbd> | <kbd>←</kbd> / <kbd>→</kbd> |
| Close the topmost dialog or menu | <kbd>Esc</kbd> | <kbd>Esc</kbd> |

Every command is also in the menu bar: native on macOS, in the window on
Windows and Linux.

## Fast on real libraries

On the latest build for macOS 26.5, Apple M5, 24 GB.

| Metric | Result |
| --- | ---: |
| Collection file size | 2.2 MB |
| Launch with a 10k collection | 0.2 s warm / 0.8 s cold |
| Memory, collection open | ~78 MB footprint |
| Metadata load | 5 ms |
| Plan an 834-page sheet | 3 ms |
| Rename plan, 10k files | 20 ms |
| Thumbnail cache | 32 images |

GPUI draws the interface on the GPU (Metal on macOS). Only visible thumbnails
are uploaded as textures; image decoding, Drive transfers, PDF export and EXIF
reads run on bounded background workers.

Large imports read file and EXIF metadata first. Thumbnails decode only when a
cell becomes visible. PDF pages stream to disk one at a time. JPEGs, CMYK JPEGs
and ordinary RGB/gray PNGs are embedded from their original compressed streams;
HEIC/RAW use in-process ImageIO on macOS. Mirrored EXIF, user rotation and
layout rotation are expressed as PDF matrices. Remaining alpha/palette/16-bit
PNGs and other formats decode in parallel. The photo store is chunked
copy-on-write: a 500k-photo collection mutation touches one chunk, snapshots
share the rest, and selection is a compact bitmap. A 500k synthetic library is
about 71 MB before thumbnails and view caches; `Photo` is 80 bytes.

Reproduce the core numbers with:

```sh
cargo run --release -p slate-core --example stress
cargo run --release -p slate-core --example stress -- --photos 500000 --gate
```

CI runs the 500k gate on Linux. It enforces the collection file size, load time,
page planning, rename planning and base resident set.

## Get started

Download the latest build from [Releases](https://github.com/et0and/slate/releases):
a disk image for macOS, an `.exe` for Windows and an AppImage for Linux. Or
build Slate with Rust 1.99.1. It runs on macOS 11+, Windows 10/11, and Linux
with X11/Wayland.

```sh
cargo run --release
cargo run --release -- ~/Pictures/Shoot
```

Then:

1. Open a folder with <kbd>⌘O</kbd>, drop photos onto the window, or press
   **+** in the Folders pane for a local or Google Drive folder.
2. Cull: rate, label and tag, and narrow the grid with search and filters.
3. Rename or convert the selects if you need to.
4. Choose **File > Create contact sheet**, tune the layout and export the PDF.

Save with <kbd>⌘S</kbd> at any point; Slate writes `collection.slate` into the
folder.

Verify:

```sh
cargo test --workspace
cargo lint
cargo fmt-check
```

`cargo lint` runs Clippy across every target with `all`, `pedantic` and
`nursery` enabled, and denies warnings. `cargo lint-fix` applies Clippy's safe
fixes. CI runs the same commands.

## Telemetry

Axiom OTLP tracing is optional, and is in place to allow me to debug and improve Slate. See [docs/telemetry.md](docs/telemetry.md) for more information.

## License and credits

Slate is licensed under the
[GNU General Public License v3.0](LICENSE). The Google Drive client uses the MIT-licensed `google-drive`
crate.
