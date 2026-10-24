# Repository guide

Slate Desktop: a cross-platform desktop contact-sheet builder in Rust + GPUI 0.2.2.

## Commands

- `cargo run --release` — launch
- `cargo run --release -- path/to/file.contactsheet` — launch with a project
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
  enabling Apply rename. Never add a clobbering rename path.
- PDFs are flat, RGB, no layers. Images are capped at 300 DPI for their cell.
- HEIC/RAW decoding goes through macOS `sips`; keep the fallback path working on
  other platforms by returning an error that the UI can display.
- `vendor/xattr` is a patched 0.2.3 crate: libc 0.2.191 removed `ENOATTR` on
  Linux. Remove the patch when `zed-async-tar` stops pulling xattr 0.2.

## Style

Monochrome and high contrast: true black and white with neutral greys, 1 px
rules, no rounded corners, no colour accent and no decorative gradients.
Restrained shadows are allowed only for the paper preview and modal. Light and
dark modes live in `theme.rs` (`set_dark`/`is_dark`); the sheet preview and PDF
stay paper-white via the `sheet_*` tokens.

Sentence case everywhere. Do not add uppercase copy, `to_uppercase()` calls or
CSS `text-transform: uppercase`. Acronyms such as PDF, RGB and DPI are fine.
Use the platform UI/mono font constants. Dense, not sparse. The masthead
starts with the project name; do not add a wordmark, logo or tagline.

Use `theme.rs` tokens and `widgets.rs` controls instead of one-off styles.

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
