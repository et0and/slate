# README imagery

- `example-sheet.jpg` and `example-rotated.jpg` are real PDF exports produced by
  `cargo run -p slate-core --example sample_sheet` from the Unsplash photographs
  in `assets/sample`.
- `app-preview-light.jpg` and `app-preview-dark.jpg` are interface previews
  rendered from `ui-preview.html`, which mirrors the shipped GPUI layout and
  theme. `menu-preview.jpg` is the same preview in One Dark with the View menu
  open. They are not live window captures: the macOS session used to build
  this repository was locked, so macOS screen capture was unavailable.

The preview HTML reads photographs from `assets/sample` and the PDF thumbnail
from `target/sample`. Create the thumbnail first:

```sh
cargo run -p slate-core --example sample_sheet
qlmanage -t -s 1400 -o target/sample target/sample/sample.pdf
```

The HTML loads the bundled Geist faces from `assets/fonts`. Regenerate the
interface preview with Chrome:

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=2 --window-size=1440,1120 \
  --allow-file-access-from-files \
  --screenshot=/tmp/slate-light.png \
  file://$PWD/docs/ui-preview.html

"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=2 --window-size=1440,1120 \
  --allow-file-access-from-files \
  --screenshot=/tmp/slate-dark.png \
  "file://$PWD/docs/ui-preview.html?theme=dark"

sips -s format jpeg -s formatOptions 85 -Z 1800 \
  /tmp/slate-light.png --out docs/app-preview-light.jpg
sips -s format jpeg -s formatOptions 85 -Z 1800 \
  /tmp/slate-dark.png --out docs/app-preview-dark.jpg
```

Capture the open View menu in One Dark:

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=2 --window-size=1440,1120 \
  --allow-file-access-from-files \
  --screenshot=/tmp/slate-menu.png \
  "file://$PWD/docs/ui-preview.html?theme=one-dark&menu=view"

sips -s format jpeg -s formatOptions 85 -Z 1800 \
  /tmp/slate-menu.png --out docs/menu-preview.jpg
```

Convert the example PDFs to the README JPEGs:

```sh
qlmanage -t -s 1400 -o target/sample target/sample/sample-rotated.pdf
sips -s format jpeg -s formatOptions 88 \
  target/sample/sample.pdf.png --out docs/example-sheet.jpg
sips -s format jpeg -s formatOptions 88 \
  target/sample/sample-rotated.pdf.png --out docs/example-rotated.jpg
```

## Interface prototype

`docs/parity-preview.html` mirrors the ratings, labels, sorting, photo inspector,
sheet builder, full-screen preview and conversion dialog. Open it with
`?view=sheet`, `?view=full`, `?view=convert` or `?menu=file`. The rendered
screenshots are `docs/parity-photo-inspector.jpg`, `parity-sheet-builder.jpg`,
`parity-fullscreen.jpg`, `parity-file-menu.jpg` and `parity-convert.jpg`.
