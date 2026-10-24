# Slate Desktop

Contact sheets for photographers. Rust + GPUI.

![Slate Desktop in light mode](docs/app-preview-light.jpg)

![Slate Desktop in dark mode](docs/app-preview-dark.jpg)

![The View menu in One Dark](docs/menu-preview.jpg)

*Light, Dark, and the View menu in One Dark.*

## Core

- Import folders, files, or drops. Tag, search, filter, batch rename, and
  remove from the project without touching the originals.
- Per-photo rotation from each thumbnail, left of the selection tick.
- Live contact-sheet layout: paper, orientation, grid, margins, gutters,
  captions, index numbers, rotate-to-fit, header and footer.
- Flat PDF export at up to 300 DPI per cell.
- Undo and redo for project edits; file renames clear the history.
- Menubar-only commands: File, Edit, View, History, Help. No duplicate action
  buttons in the panels.
- Themes under View: Light, Dark, One Dark, Monokai, Catppuccin.
- `.slate` project files and saved sheets. A local SQLite catalogue powers
  recent projects.
- Resizable rails and a virtualised grid for large libraries.
- macOS HEIC/RAW import through ImageIO; standard formats elsewhere.

## Performance

10,000 photos, A4 portrait, 3 × 4, one image per cell. Release build on macOS
26.5, Apple M5, 24 GB.

| Metric | Result |
| --- | ---: |
| Project file | 2.2 MB |
| Launch to loaded project | 0.18 s warm / 0.77 s cold |
| Resident memory, project loaded | ~137 MB |
| Metadata load | 4 ms |
| Plan 834 pages | 3 ms |
| Thumbnail cache | 256 images |

The grid is virtualised: only visible rows render and only visible thumbnails
load. PDF export is on demand and pixel-bound; those 10,000 photos become 834 A4
pages at 12 up.

The stress example writes the 10,000-photo project used for the metadata and
planning rows.

Reproduce the core side with:

```sh
cargo run --release -p slate-core --example stress
```

## Sheet output

![Example contact sheet](docs/example-sheet.jpg)

*A4 portrait, 3 × 4, 12 photos.*

![Rotated contact sheet](docs/example-rotated.jpg)

*A4 landscape, 4 × 3, rotate-to-fit.*

## Run

Rust 1.99.1. macOS 11+, Windows 10/11, or Linux with X11/Wayland.

```sh
cargo run --release
cargo run --release -- ~/Pictures/project.slate
```

Build and verify:

```sh
cargo build --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## License and credits

MIT. Bundled Geist and Geist Mono are under the SIL Open Font License. Sample
photographs are from Unsplash. SQLite is public domain.
