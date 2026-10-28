# Capture — notes for working on this app

- GTK4 (`gtk4` 0.11, `v4_18`) + Rust. No libadwaita. Nyx Design 2.0
  (`nyx_theme::use_design_2`, see nyx-design `STYLE.md`). The history window: one
  unified header (`chrome::app_header`) with the page as its subtitle and the sidebar
  toggle, search in the middle (the selection's count and actions while captures are
  selected), the count, the capture and recording actions and Settings at the end; a
  flat sidebar (`hexpand` false, icons under 980 px) and the grid. No status bar. The
  editor has the same header (the shot's title; undo, redo, redact, more, save, copy)
  with the tools centred under the shot. Settings is `nyx_ui::settings_card` (groups in
  `settings_dialog::GROUPS`); its Accent row replaces Glow. Colors are `@theme_*`
  tokens (cairo reads `theme::palette()`).
- `crates/nyx-capture-core` owns grim capture, slurp, hyprctl geometry (windows only on
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
  copy, pin and Nyx Assistant; `Db::thumb` for thumbnails (remade when older than the
  sidecar). Blur and pixelate have a floor (`MIN_OBSCURE`); Redact uses solid `Fill` boxes.
- History is a virtualized `ListView` of `grid::Line`s: day headers and rows of cells,
  rebuilt when the width changes the columns. `grid` keeps the selection by id. Deleting
  hides rows (`window::PENDING`) and removes them when the Undo toast has gone, or at
  close/shutdown (`flush_deletes`). Thumbs live in `~/.cache/nyx-capture/thumbs`
  (`{id}.jpg`, recorded in `thumb_path`). Retention prunes unstarred screenshots on
  launch; recordings (saved in Videos) aren't pruned. Schema 2 turned old pinned rows
  into starred ones. History off: nothing is inserted (scroll captures go to the save
  folder). Hotkey (cold) captures are inserted by `remember_cold`, their OCR by a detached
  `nyx-capture --ocr-id ID`. Titles come from the focused window at capture start.
- Cold capture (`--region`, `--window`) builds the overlay only. `--fullscreen` skips GTK.
  slurp is the fallback when gtk4-layer-shell is unsupported. The window hides (Hyprland
  doesn't minimize) while capturing.
- Recording wraps gpu-screen-recorder / wf-recorder / wl-screenrec / gst (portal). Stored
  pids are checked against `/proc/<pid>/comm` before any signal; `record::ended()` hands
  back a recording whose recorder died. Webcam overlay is mpv `WebcamOverlay`.
- Pins use class `io.github.design_nexus.nyx.Capture.Pin` and hold the application.
- Control tools: `capture.region`, `capture.window`, `capture.fullscreen`, `capture.search`,
  `capture.open`, `capture.ocr_text`, `capture.export`, `capture.record_start`,
  `capture.record_stop`, `capture.record_pause`, `capture.send_screenshot` (gated by
  `assistant_can_send`; refuses sensitive rows; edited shots go out flattened).
- `apps/capture-glimpse` searches `capture.sqlite` read-only (immutable when the WAL files
  can't be made). History hits copy OCR text. `capture region` / `window` / `screen` return
  activate tokens; Glimpse launches `nyx-capture --region` (etc.) on the host.
- Layout checks: run a test copy under `NYX_CAPTURE_APP_ID=io.github.design_nexus.nyx.CaptureTest`
  with scratch `XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_CACHE_HOME` on a virtual monitor of
  its own; `--monitor <that output> --edit` fills the scratch history and opens the
  editor. Pins and the overlay keep their own classes, so don't open them from a test copy.
- Checks: `cargo test -p nyx-capture-core -p nyx-capture -p nyx-capture-glimpse`
  and clippy `-D warnings` on those packages.
