<img width="1090" height="481" alt="slate" src="https://github.com/user-attachments/assets/73da4be4-91fc-4fbe-8e07-505b6420fdfe" />

<p align="center">Photo management for the rest of us</p>

<p align="center">
  <a href="https://tom.so/work/slate">Website</a>
  ·
  <a href="https://github.com/et0and/slate">Source</a>
  ·
  <a href="https://github.com/et0and/slate/releases">Releases</a>
  ·
  <a href="https://github.com/et0and/slate/issues">Issues</a>
  ·
  <a href="docs/README.md">Docs</a>
</p>

<img width="1920" height="1080" alt="Screenshot 2026-10-25 at 11 55 04 AM" src="https://github.com/user-attachments/assets/b2caa100-2a02-4e30-9c0f-cbb2acb3ba8e" />

Slate turns a folder of photos into a print-ready contact sheet. Local-first, with Drive optional. Import a shoot, tag selects, rotate frames, and export a flat PDF. The `.slate` project stays portable; the photos stay where they are.

Use Slate for client proofs, portfolio edits, archive indexes, and the first pass through a shoot.

## Why Slate

- **Built for the cull.** Import folders or drops, search by name, tag, drag-select, right-click to rename, and rotate each frame from its thumbnail.
- **Fast on real libraries.** The grid is virtualised and only visible thumbnails load. A 10,000-photo project opens in about 0.2 seconds warm and stays around 78 MB.
- **Your files stay yours.** Local photos are read in place; Drive imports download beside the project. `.slate` files hold layout, tags, rotation, and saved sheets — not image data.
- **One sheet, no noise.** A native macOS menu bar, eleven themes, zero toolbars, and collapsible side panes. Commands live in File, Edit, View, Drive, and Help.
- **Dates in names.** Use `{date}`, `{time}`, `{datetime}` or `{year}` tokens in the rename prefix and suffix. Right-click any frame for properties, EXIF, reveal and path actions.
- **Contact sheet automation is not good enough.** On low-powered machines, the Photoshop batch workflow is [buggy and resource intensive](https://www.reddit.com/r/photoshop/comments/vpwwn7/attempting_to_create_a_contact_sheet_that_uses/).
- **Print-ready by default.** A4, A3, Letter, Tabloid, or a custom size export as flat PDFs at up to 300 DPI per cell.
- **Undo that keeps up.** Every layout change is undoable without copying the whole library.

## Benchmark

On the latest build for macOS 26.5, Apple M5, 24 GB.

| Metric | Result |
| --- | ---: |
| Project file size | 2.2 MB |
| Launch with a 10k project | 0.2 s warm / 0.8 s cold |
| Memory, project open | ~78 MB footprint |
| Metadata load | 5 ms |
| Plan an 834-page sheet | 3 ms |
| Rename plan, 10k files | 20 ms |
| Thumbnail cache | 32 images |

## Themes

Slate supports popular colour themes, with support for custom themes coming soon.

<img src="docs/app-preview-dark.jpg" width="49%" alt="Slate in dark mode"> <img src="docs/menu-preview.jpg" width="49%" alt="The View menu in One Dark">

GPUI draws the interface on the GPU (Metal on macOS). Only visible thumbnails are uploaded as textures; image decoding, Drive transfers, PDF export and EXIF reads run on bounded background workers.

Large imports read file and EXIF metadata first. Thumbnails decode only when a cell becomes visible. PDF pages stream to disk one at a time, and photo edits use field-level undo deltas instead of copying the whole library.

Reproduce the core numbers with:

```sh
cargo run --release -p slate-core --example stress
```

## Output

![Example contact sheet](docs/example-sheet.jpg)

*A4 portrait, 3 × 4, 12 photos.*

![Rotated contact sheet](docs/example-rotated.jpg)

*A4 landscape, 4 × 3, rotate-to-fit.*

## Google Drive

Link a Drive account from the Drive menu to browse folders, import images next to the `.slate` project, and export a PDF to a folder you choose. Setup takes one Desktop OAuth client: [Google Drive setup](docs/google-drive.md).

## Get started

Download the latest build from [Releases](https://github.com/et0and/slate/releases), or build Slate with Rust 1.99.1. It runs on macOS 11+, Windows 10/11, and Linux with X11/Wayland.

```sh
cargo run --release
cargo run --release -- ~/Pictures/project.slate
```

Then:

1. Drop in a folder of photos.
2. Tag selects, rotate frames, and tune the sheet.
3. Save the `.slate` project.
4. Export the PDF.

Verify:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Releases

Every merge to `main` is versioned from Conventional Commits and published as a GitHub Release with macOS, Linux and Windows builds. See [docs/release.md](docs/release.md) for the version rules and macOS notarization.

## License and credits

Slate is licensed under the [GNU General Public License v3.0](LICENSE). Bundled Geist and Geist Mono are under the SIL Open Font License. Sample photographs are from Unsplash. SQLite is public domain. The Google Drive client uses the MIT-licensed `google-drive` crate.
