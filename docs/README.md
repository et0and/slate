# README imagery

- `example-sheet.jpg` and `example-rotated.jpg` are real PDF exports produced by
  `cargo run -p slate-core --example sample_sheet`.
- `app-preview.jpg` is an interface preview rendered from `ui-preview.html`,
  which mirrors the shipped GPUI layout and theme. It is not a live window
  capture: the macOS session used to build this repository was locked, so
  macOS screen capture was unavailable.

To regenerate the interface preview with Chrome:

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=2 --window-size=1440,1060 \
  --allow-file-access-from-files \
  --screenshot=/tmp/slate-ui.png \
  file://$PWD/docs/ui-preview.html
sips -s format jpeg -s formatOptions 85 -Z 1800 \
  /tmp/slate-ui.png --out docs/app-preview.jpg
```
