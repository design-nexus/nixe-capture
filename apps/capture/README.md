# Nixe Capture

Screenshots, annotation, OCR, and screen recording for Omarchy / Hyprland.

```
nixe-capture                 # history window
nixe-capture --region        # drag a region, copy the PNG
nixe-capture --window
nixe-capture --active-window
nixe-capture --fullscreen
nixe-capture --ocr           # region, then copy text
nixe-capture --pin           # region, then pin
nixe-capture --region --edit # region, then the annotation editor
nixe-capture --copy|--save [PATH]
nixe-capture --record        # focused monitor; --region to pick; --stop / --pause / --webcam
```

Needs `grim`. Region pick uses a GTK layer-shell overlay (magnifier, dimensions, window snap) and falls back to `slurp` when layer-shell is missing. After a shot, the annotation editor copies a flattened PNG and leaves the original file unchanged.

OCR defaults to Tesseract. Automatic mode falls back to RapidOCR when Tesseract's quality band is poor or empty. RapidOCR models load on first fallback, never at startup.

Recording wraps `gpu-screen-recorder`, then `wf-recorder`, then `wl-screenrec`, then `gst-launch-1.0` after the XDG ScreenCast portal. Settings only show those rows when a recorder is on PATH. Ctrl+R starts or stops; Ctrl+Shift+R records a region; Ctrl+Shift+P pauses on gpu-screen-recorder. Webcam overlay is mpv with Omarchy `WebcamOverlay` window rules. History can trim a recording with ffmpeg.

`nixe-capture-glimpse` searches history OCR (copy text) and the commands `capture region`, `capture window`, and `capture screen`. Glimpse launches `nixe-capture --region` (or `--window` / `--fullscreen`) on the host; the plugin cannot spawn GTK. The plugin still copies that CLI if an older Glimpse handles activate.

Suggested Omarchy/Hyprland binds (keep them in your hypr config; install does not write them):

```lua
o.bind("SUPER + SHIFT + S", "Capture region", "nixe-capture --region")
o.bind("SUPER + SHIFT + W", "Capture window", "nixe-capture --window")
o.bind("PRINT", "Capture screen", "nixe-capture --fullscreen")
o.bind("SUPER + SHIFT + O", "Capture text", "nixe-capture --ocr")
```

See [PLAN.md](PLAN.md).
