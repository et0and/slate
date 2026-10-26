# Video fixtures

Generated from FFmpeg's built-in test pattern, so they carry no third-party
footage:

```sh
ffmpeg -f lavfi -i testsrc=duration=4:size=160x120:rate=10 \
  -c:v libx264 -pix_fmt yuv420p -g 10 -movflags +faststart clip.mp4
ffmpeg -display_rotation -90 -i clip.mp4 -c copy -movflags +faststart clip-portrait.mp4
```

`clip-portrait.mp4` carries the display matrix a phone writes for portrait
video, which decodes upright at 120 × 160.
