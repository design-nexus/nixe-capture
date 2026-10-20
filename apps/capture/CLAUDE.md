# Capture — notes for working on this app

- GTK4 (`gtk4` 0.11, `v4_18`) + Rust. No libadwaita. The history window follows
  Music: a flat sidebar (`hexpand` false, 250 px, icons under 980 px), the page, and a
  status bar. Settings is `nixe_ui::settings_card`. Colors are `@theme_*` tokens.
- `crates/nixe-capture-core` owns grim capture, slurp, hyprctl geometry, dual OCR
  (Tesseract default, RapidOCR lazy fallback), SQLite history, and annotation objects
  (flatten in `annot::render`). The app crate is the GTK shell, overlay, editor, pin
  windows, history grid, and OCR view. The original PNG is never overwritten;
  `{id}.ann.json` stores `EditState`. Pins use class `io.github.design_nexus.nixe.Capture.Pin`.
  History is a virtualized `GridView`; thumbs live in `~/.cache/nixe-capture/thumbs`. Retention
  prunes unstarred rows on launch. History off skips inserts except an explicit Save.
- Cold capture (`--region`, `--window`) builds the overlay only. `--fullscreen` skips GTK.
  slurp is the fallback when gtk4-layer-shell is unsupported.
- RapidOCR is a lazy fallback and is not loaded at startup. Recording wraps gpu-screen-recorder / wf-recorder / wl-screenrec. Webcam overlay is mpv `WebcamOverlay`.
- Control tools: `capture.region`, `capture.window`, `capture.fullscreen`, `capture.search`, `capture.open`, `capture.ocr_text`, `capture.export`, `capture.record_start`, `capture.record_stop`, `capture.record_pause`.
- `apps/capture-glimpse` searches `capture.sqlite` read-only. History hits copy OCR text. `capture region` / `window` / `screen` return activate tokens. Glimpse launches `nixe-capture --region` (etc.) on the host; the plugin's `activate` copies that CLI as a fallback.
- Layout stills: `CAPTURE_SNAPSHOT=/tmp/capture.png nixe-capture`.
- Checks: `cargo test -p nixe-capture-core -p nixe-capture -p nixe-capture-glimpse`
  and clippy `-D warnings` on those packages.
