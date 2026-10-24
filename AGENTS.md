# Repository guide

Slate Desktop: a desktop contact-sheet builder in Rust + GPUI 0.2.2, macOS-first.

## Commands

- `cargo run --release` — launch
- `cargo run --release -- path/to/file.contactsheet` — launch with a project
- `cargo test --workspace` — core + app tests
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all`
- `cargo run -p slate-core --example sample_sheet` — regenerate `target/sample`
  and the design-check PDFs

## Shape

```
crates/slate-core   GUI-free: model, import, batch, layout, pdf
crates/slate-app    GPUI shell: app state, text field, panels, theme
```

- One file per domain in core. Do not add GPUI dependencies to `slate-core`.
- `slate-app` owns all dialogs, async tasks, thumbnails and interaction state.
- Keep the app single-window unless the whole state model is reworked.

## Rules

- Sheet geometry must come from `layout::plan_sheet`; the preview and PDF must
  never compute their own cells.
- New project fields need `#[serde(default)]` and `Project::normalize` coverage
  so old `.contactsheet` files keep opening.
- Async project replacement increments `generation`; every thumbnail, import and
  rotation task checks it before touching state.
- Keep background concurrency bounded: 3 import/thumbnail jobs, 2 rotation jobs.
- Text field callbacks must not read or update the same field from inside its
  own event handler (GPUI entity re-borrow panic). Pass values through the
  callback instead.
- Rename targets are checked against existing files and duplicate targets before
  enabling APPLY. Never add a clobbering rename path.
- PDFs are flat, RGB, no layers. Images are capped at 300 DPI for their cell.
- HEIC/RAW decoding goes through macOS `sips`; keep the fallback path working on
  other platforms by returning an error that the UI can display.

## Style

Match the existing U.S. Graphics direction: flat surfaces, 1 px rules, sharp
corners, no shadows or gradients, warm paper `#FBFAF6`, ink `#111110`, one
vermilion accent `#E43D0F`. All-caps mono micro labels, Helvetica Neue for
titles. Dense, not sparse.

Use `theme.rs` tokens and `widgets.rs` controls instead of one-off styles.

## Tests

- Core: layout geometry/pagination/rotation, rename conflicts and apply,
  tag normalisation, project JSON roundtrip, PDF generation with real images,
  missing-file placeholders.
- App: pure helpers such as `slug`.
- Visual output: run `sample_sheet`, convert the PDF with
  `qlmanage -t -s 1400 -o target/sample target/sample/sample.pdf`, and inspect.
