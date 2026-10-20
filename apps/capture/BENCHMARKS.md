# Capture benchmarks

Measured 2026-10-19 on this Omarchy/Hyprland box (eDP-2 2560×1600 @ 1.6 scale, grim 1.5.0, tesseract 5.5.3).

| Step | Time |
| --- | --- |
| `grim -t png` fullscreen | 85 ms |
| `nixe-capture --fullscreen --save` (release) | 141 ms |
| Tesseract `--psm 6` on that PNG | ~2.5 s |
| Tesseract TSV on that PNG | ~2.8 s |

Notes:

- Save path keeps grim's PNG bytes (no re-encode). Decode still happens so OCR and the editor can use pixels.
- RapidOCR was not loaded. Overlay selector crops the grim still; slurp is the fallback.
- Overlay budget from the plan is < 150 ms *after grim returns*. Cold binary + grim + write is already in that neighbourhood.
