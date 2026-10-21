# Capture — notes for working on this app

- GTK4 (`gtk4` 0.11, `v4_18`) + Rust. No libadwaita. The history window follows
  Music: a flat sidebar (`hexpand` false, 250 px, icons under 980 px), the page, and a
  status bar. Settings is `nixe_ui::settings_card` (groups in `settings_dialog::GROUPS`).
  Colors are `@theme_*` tokens (cairo reads `theme::palette()`).
- `crates/nixe-capture-core` owns grim capture, slurp, hyprctl geometry (windows only on
  visible workspaces; transforms 1/3/5/7 swap sides), dual OCR (Tesseract default, words
  grouped by block/paragraph/line; RapidOCR lazy fallback, models only when present or
  downloaded from Settings), SQLite history, and annotation objects. The app crate is the
  GTK shell, overlay, recording pill, editor, pin windows, history grid, and OCR view.
- Annotations: the original PNG is never overwritten; `{id}.ann.json` stores `EditState`.
  Marks and the crop are in the whole picture's normalized terms (`STATE_VERSION` 1;
  `render` draws marks, then crops). Version-0 sidecars (marks in cropped terms) still
  render as before and are `upgraded` when opened in the editor. The editor canvas shows
  the crop, so new marks go through `placed()` / `picture_point()`.
- Anything that leaves the editor uses the edited picture: `export::shareable(_png)` for
  copy, pin and Nixe Assistant; `Db::thumb` for thumbnails (remade when older than the
  sidecar). Blur and pixelate have a floor (`MIN_OBSCURE`); Redact uses solid `Fill` boxes.
- History is a virtualized `GridView`; thumbs live in `~/.cache/nixe-capture/thumbs`
  (`{id}.jpg`, recorded in `thumb_path`). Retention prunes unstarred screenshots on
  launch; recordings (saved in Videos) aren't pruned. Schema 2 turned old pinned rows
  into starred ones. History off: nothing is inserted (scroll captures go to the save
  folder). Hotkey (cold) captures are inserted by `remember_cold`, their OCR by a detached
  `nixe-capture --ocr-id ID`. Titles come from the focused window at capture start.
- Cold capture (`--region`, `--window`) builds the overlay only. `--fullscreen` skips GTK.
  slurp is the fallback when gtk4-layer-shell is unsupported. The window hides (Hyprland
  doesn't minimize) while capturing.
- Recording wraps gpu-screen-recorder / wf-recorder / wl-screenrec / gst (portal). Stored
  pids are checked against `/proc/<pid>/comm` before any signal; `record::ended()` hands
  back a recording whose recorder died. Webcam overlay is mpv `WebcamOverlay`.
- Pins use class `io.github.design_nexus.nixe.Capture.Pin` and hold the application.
- Control tools: `capture.region`, `capture.window`, `capture.fullscreen`, `capture.search`,
  `capture.open`, `capture.ocr_text`, `capture.export`, `capture.record_start`,
  `capture.record_stop`, `capture.record_pause`, `capture.send_screenshot` (gated by
  `assistant_can_send`; refuses sensitive rows; edited shots go out flattened).
- `apps/capture-glimpse` searches `capture.sqlite` read-only (immutable when the WAL files
  can't be made). History hits copy OCR text. `capture region` / `window` / `screen` return
  activate tokens; Glimpse launches `nixe-capture --region` (etc.) on the host.
- Layout stills: `CAPTURE_SNAPSHOT=/tmp/capture.png nixe-capture`.
- Checks: `cargo test -p nixe-capture-core -p nixe-capture -p nixe-capture-glimpse`
  and clippy `-D warnings` on those packages.
