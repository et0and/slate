# README imagery

- `example-sheet.jpg` and `example-rotated.jpg` are real PDF exports produced by
  `cargo run -p slate-core --example sample_sheet`.
- `app-preview-light.jpg` and `app-preview-dark.jpg` are interface previews
  rendered from `ui-preview.html`, which mirrors the shipped GPUI layout and
  theme. They are not live window captures: the macOS session used to build
  this repository was locked, so macOS screen capture was unavailable.

To regenerate the interface preview with Chrome:

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
