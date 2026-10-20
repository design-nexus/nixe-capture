//! The Capture window. The sidebar follows Music: it does not grow, and it
//! collapses to icons under 980 px or when the user collapses it.

use crate::paths;
use crate::prefs;
use crate::settings_dialog;
use crate::widgets;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use nixe_capture_core::action::{self, Action, After};
use nixe_capture_core::annot;
use nixe_capture_core::capture::CaptureMode;
use nixe_capture_core::clipboard;
use nixe_capture_core::ocr::{OcrMode, recognize};
use nixe_capture_core::record::{self, Container, RecordRequest, Target};
use nixe_capture_core::search::View;
use nixe_capture_core::store::{self, CaptureKind, CaptureRow};
use nixe_ui::chrome;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

pub const SHORTCUTS: &[(&[&str], &str)] = &[
    (&["Ctrl", "Shift", "L"], "Scrolling capture"),
    (&["Ctrl", "N"], "Capture a region"),
    (&["Ctrl", "Shift", "N"], "Capture a window"),
    (&["Ctrl", "Shift", "F"], "Capture the focused monitor"),
    (&["Ctrl", "R"], "Start or stop a recording"),
    (&["Ctrl", "Shift", "R"], "Record a region"),
    (&["Ctrl", "Shift", "P"], "Pause or resume a recording"),
    (&["Ctrl", "F"], "Search history"),
    (&["Ctrl", "B"], "Collapse or expand sidebar"),
    (&["Ctrl", ","], "Settings"),
    (&["F1"], "Show keyboard shortcuts"),
    (&["Ctrl", "Q"], "Close"),
];

struct Ui {
    window: gtk::ApplicationWindow,
    overlay: gtk::Overlay,
    nav: gtk::Box,
    nav_list: gtk::Box,
    search: gtk::SearchEntry,
    status: gtk::Label,
    hint: gtk::Label,
    scroller: gtk::ScrolledWindow,
    grid: gio::ListStore,
    view: View,
    items: Vec<CaptureRow>,
}

thread_local! {
    static UI: RefCell<Option<Rc<RefCell<Ui>>>> = const { RefCell::new(None) };
    static NARROW: Cell<bool> = const { Cell::new(false) };
    static TOAST: RefCell<Option<gtk::Box>> = const { RefCell::new(None) };
    static STATUS: RefCell<Option<gtk::Label>> = const { RefCell::new(None) };
    static MENU: RefCell<Option<gtk::Popover>> = const { RefCell::new(None) };
    static RECORD_BTN: RefCell<Option<gtk::Button>> = const { RefCell::new(None) };
    static PAUSE_BTN: RefCell<Option<gtk::Button>> = const { RefCell::new(None) };
    static CAM_BTN: RefCell<Option<gtk::Button>> = const { RefCell::new(None) };
}

fn ui() -> Option<Rc<RefCell<Ui>>> {
    UI.with(|u| u.borrow().clone())
}

pub fn window() -> Option<gtk::ApplicationWindow> {
    ui().map(|u| u.borrow().window.clone())
}

pub fn overlay() -> Option<gtk::Overlay> {
    ui().map(|u| u.borrow().overlay.clone())
}

pub fn present(app: &gtk::Application, section: Option<&str>) {
    if ui().is_none() {
        build(app);
    }
    let Some(ui) = ui() else { return };
    let window = ui.borrow().window.clone();
    if std::env::var_os("CAPTURE_SNAPSHOT").is_some() {
        window.set_default_size(
            std::env::var("CAPTURE_SNAPSHOT_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1120),
            std::env::var("CAPTURE_SNAPSHOT_H").ok().and_then(|v| v.parse().ok()).unwrap_or(800),
        );
    }
    window.present();
    match section {
        Some("settings") => settings_dialog::open(),
        Some(other) => navigate(view_from_section(other)),
        None => {
            let start = nixe_theme::start_page("recent", &prefs::get().last_section);
            navigate(view_from_section(&start));
        }
    }
    if let Some(out) = std::env::var_os("CAPTURE_SNAPSHOT") {
        snapshot_and_quit(app, PathBuf::from(out));
        return;
    }
    maybe_tutorial();
}

fn view_from_section(section: &str) -> View {
    match section {
        "starred" => View::Starred,
        "screenshots" => View::Screenshots,
        "recordings" => View::Recordings,
        _ => View::Recent,
    }
}

fn section_id(view: View) -> &'static str {
    match view {
        View::Recent => "recent",
        View::Screenshots => "screenshots",
        View::Recordings => "recordings",
        View::Starred => "starred",
    }
}

fn page_name(view: View) -> &'static str {
    match view {
        View::Recent => "Recent",
        View::Screenshots => "Screenshots",
        View::Recordings => "Recordings",
        View::Starred => "Starred",
    }
}

pub fn navigate(view: View) {
    let Some(ui) = ui() else { return };
    ui.borrow_mut().view = view;
    prefs::update(|p| p.last_section = section_id(view).into());
    chrome::set_title(&ui.borrow().window, "Capture", page_name(view));
    reload();
    rebuild_nav();
}

pub fn reload() {
    let Some(ui) = ui() else { return };
    let view = ui.borrow().view;
    let query = ui.borrow().search.text().to_string();
    if !prefs::get().history {
        ui.borrow_mut().items.clear();
        crate::grid::fill(&ui.borrow().grid, &[]);
        ui.borrow().scroller.set_visible(false);
        ui.borrow().hint.set_visible(true);
        ui.borrow().hint.set_text("History is off. New shots still copy or save when you ask.");
        refresh_status();
        return;
    }
    match store::open(&paths::library_db()).and_then(|db| db.list(view, query.trim(), 400)) {
        Ok(items) => {
            let empty = items.is_empty();
            crate::grid::fill(&ui.borrow().grid, &items);
            ui.borrow_mut().items = items;
            ui.borrow().scroller.set_visible(!empty);
            ui.borrow().hint.set_visible(empty);
            ui.borrow().hint.set_text(if empty { empty_copy(view) } else { "" });
            refresh_status();
        }
        Err(err) => toast(&err.to_string()),
    }
}

fn empty_copy(view: View) -> &'static str {
    match view {
        View::Recordings => "No recordings yet. Ctrl+R records the focused monitor.",
        View::Starred => "Star a capture from the grid.",
        View::Screenshots => "No screenshots yet. Ctrl+N grabs a region.",
        View::Recent => "No captures yet. Ctrl+N grabs a region.",
    }
}

pub fn search(query: &str) -> anyhow::Result<Value> {
    let db = store::open(&paths::library_db())?;
    let items = db.list(View::Recent, query, 20)?;
    Ok(json!(items
        .into_iter()
        .map(|c| json!({"id": c.id, "title": c.title, "path": c.path.display().to_string()}))
        .collect::<Vec<_>>()))
}

pub fn send_screenshot(id: &str) -> Result<Value, String> {
    let db = store::open(&paths::library_db()).map_err(|e| e.to_string())?;
    let row = db.get(id).map_err(|e| e.to_string())?.ok_or_else(|| "that capture is not in the library".to_string())?;
    if row.sensitive {
        return Err("that capture is marked sensitive".into());
    }
    let text = db.ocr_text(id).map_err(|e| e.to_string())?.unwrap_or_default();
    Ok(json!({"id": id, "text": text, "path": row.path.display().to_string()}))
}

pub fn start_scroll() {
    if record::active().is_some() {
        toast("Stop the recording first");
        return;
    }
    if !nixe_capture_core::scroll::available() {
        toast("Scrolling capture needs omasnap");
        return;
    }
    toast("Scroll the region in omasnap");
    let dir = paths::captures_dir();
    crate::cmd::background(
        move || nixe_capture_core::scroll::capture(&dir),
        |result| match result {
            Ok(path) => ingest_scroll(path),
            Err(err) => toast(&err.to_string()),
        },
    );
}

fn ingest_scroll(path: PathBuf) {
    crate::cmd::background(
        move || {
            let img = image::open(&path)?.to_rgba8();
            let (w, h) = (img.width() as i32, img.height() as i32);
            let id = if prefs::get().history {
                let db = store::open(&paths::library_db())?;
                let id = db.insert(&store::NewCapture {
                    kind: CaptureKind::Screenshot,
                    width: w,
                    height: h,
                    path: path.clone(),
                    source: "omasnap".into(),
                })?;
                let _ = store::write_thumb(&path, &paths::thumb_file(&id), store::THUMB_EDGE);
                Some(id)
            } else {
                None
            };
            anyhow::Ok((img, id, path))
        },
        |result| match result {
            Ok((img, id, path)) => {
                reload();
                crate::editor::open_saved(img, annot::EditState::default(), id.unwrap_or_default(), path);
            }
            Err(err) => toast(&err.to_string()),
        },
    );
}

pub fn show_tutorial() {
    let (dialog, card) = widgets::dialog("Capture tutorial", 460);
    let note = widgets::label("1. Region (or Scroll) freezes the screen.\n2. Annotate, then Redact if the shot has secrets.\n3. Copy leaves the original file unchanged. Esc cancels.", "dim");
    note.set_wrap(true);
    card.append(&note);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let later = gtk::Button::with_label("Not now");
    let go = gtk::Button::with_label("Capture a region");
    go.add_css_class("suggested-action");
    buttons.append(&later);
    buttons.append(&go);
    card.append(&buttons);
    let d = dialog.clone();
    later.connect_clicked(move |_| {
        prefs::update(|p| p.tutorial_done = true);
        d.close();
    });
    let d = dialog.clone();
    go.connect_clicked(move |_| {
        prefs::update(|p| p.tutorial_done = true);
        d.close();
        capture_now(CaptureMode::SelectRegion);
    });
    dialog.present();
}

fn maybe_tutorial() {
    if prefs::get().tutorial_done {
        return;
    }
    show_tutorial();
}

pub fn ocr_text(id: &str) -> anyhow::Result<Value> {
    let db = store::open(&paths::library_db())?;
    let row = db.get(id)?.ok_or_else(|| anyhow::anyhow!("that capture is not in the library"))?;
    if row.sensitive {
        anyhow::bail!("that capture is marked sensitive");
    }
    let text = db.ocr_text(id)?.unwrap_or_default();
    Ok(json!({"id": id, "text": text}))
}

pub fn open_id(id: &str) -> anyhow::Result<Value> {
    let db = store::open(&paths::library_db())?;
    let row = db.get(id)?.ok_or_else(|| anyhow::anyhow!("that capture is not in the library"))?;
    open_capture(row);
    Ok(json!({"opened": true, "id": id}))
}

pub fn export_id(id: &str, dest: &std::path::Path) -> anyhow::Result<Value> {
    let db = store::open(&paths::library_db())?;
    let row = db.get(id)?.ok_or_else(|| anyhow::anyhow!("that capture is not in the library"))?;
    if row.kind == CaptureKind::Recording {
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(&row.path, dest)?;
    } else {
        nixe_capture_core::export::write_flat(&row.path, dest)?;
    }
    Ok(json!({"path": dest.display().to_string(), "id": id}))
}

fn refresh_status() {
    let Some(ui) = ui() else { return };
    let count = ui.borrow().items.len();
    let mut text = if !prefs::get().history {
        "History off".into()
    } else if count == 0 {
        "Capture".into()
    } else if count == 1 {
        "1 capture".into()
    } else {
        format!("{count} captures")
    };
    if let Some(session) = record::active() {
        text = if session.paused { format!("Paused · {text}") } else { format!("Recording · {text}") };
    }
    ui.borrow().status.set_text(&text);
}

fn rebuild_nav() {
    let Some(ui) = ui() else { return };
    let list = ui.borrow().nav_list.clone();
    let current = ui.borrow().view;
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for (view, icon, label) in [
        (View::Recent, "document-open-recent-symbolic", "Recent"),
        (View::Screenshots, "camera-photo-symbolic", "Screenshots"),
        (View::Recordings, "media-record-symbolic", "Recordings"),
        (View::Starred, "starred-symbolic", "Starred"),
    ] {
        let (btn, _) = nav_button(icon, label, label);
        if view == current {
            btn.add_css_class("active");
        }
        btn.connect_clicked(move |_| navigate(view));
        list.append(&btn);
    }
    let nav = ui.borrow().nav.clone();
    apply_compact(&nav, NARROW.with(|n| n.get()) || prefs::get().sidebar_collapsed);
}

fn nav_button(icon: &str, title: &str, tooltip: &str) -> (gtk::Button, gtk::Box) {
    let btn = gtk::Button::new();
    btn.add_css_class("nav-item");
    btn.set_tooltip_text(Some(tooltip));
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&gtk::Image::from_icon_name(icon));
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.add_css_class("compact-hide");
    row.append(&label);
    btn.set_child(Some(&row));
    (btn, row)
}

fn build(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::builder().application(app).title("Capture").default_width(1120).default_height(800).build();
    window.add_css_class("capture-window");
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    window.set_icon_name(Some("io.github.design_nexus.nixe.Capture"));
    nixe_ui::keys::install(
        &window,
        nixe_ui::keys::Common {
            settings: Some(settings_dialog::open),
            shortcuts: Some(show_shortcuts),
            close: true,
            ..Default::default()
        },
    );

    let nav = gtk::Box::new(gtk::Orientation::Vertical, 0);
    nav.add_css_class("settings-navigation");
    nav.set_hexpand(false);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.set_hexpand(false);
    let nav_scroll = gtk::ScrolledWindow::builder()
        .hexpand(false)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vexpand(true)
        .child(&list)
        .build();
    let head = chrome::sidebar_head("Capture", Some("Search captures (Ctrl+F)"));
    head.toggle.connect_clicked(|_| toggle_sidebar());
    nav.append(&head.root);
    nav.append(&nav_scroll);

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search captures"));
    search.set_visible(false);
    search.connect_search_changed(|_| reload());
    search.connect_stop_search(|entry| {
        entry.set_text("");
        entry.set_visible(false);
    });
    if let Some(find) = &head.search {
        let entry = search.clone();
        find.connect_clicked(move |_| {
            if entry.is_visible() && entry.text().is_empty() {
                entry.set_visible(false);
            } else {
                entry.set_visible(true);
                entry.grab_focus();
            }
        });
    }

    let hint = widgets::label("No captures yet. Ctrl+N grabs a region.", "empty-hint");
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    hint.set_valign(gtk::Align::Start);
    hint.set_vexpand(true);

    let (scroller, grid) = crate::grid::build();

    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    toolbar.add_css_class("toolbar");
    let tools = nixe_capture_core::detect::Tools::probe();
    if tools.grim {
        let region = widgets::icon_button("capture-region-symbolic", "Capture a region (Ctrl+N)");
        region.add_css_class("suggested-action");
        region.connect_clicked(|_| capture_now(CaptureMode::SelectRegion));
        toolbar.append(&region);
        if tools.hyprctl {
            let window_btn = widgets::icon_button("focus-windows-symbolic", "Capture a window (Ctrl+Shift+N)");
            window_btn.connect_clicked(|_| capture_now(CaptureMode::SelectWindow));
            toolbar.append(&window_btn);
        }
        let screen = widgets::icon_button("video-display-symbolic", "Capture the focused monitor (Ctrl+Shift+F)");
        screen.connect_clicked(|_| capture_now(CaptureMode::FullDesktop));
        toolbar.append(&screen);
        if tools.can_scroll() {
            let scroll = widgets::icon_button("view-continuous-symbolic", "Scrolling capture (Ctrl+Shift+L)");
            scroll.connect_clicked(|_| start_scroll());
            toolbar.append(&scroll);
        }
    }
    if tools.can_record() {
        let rec = widgets::icon_button("media-record-symbolic", "Start a recording (Ctrl+R)");
        rec.connect_clicked(|_| toggle_record(false));
        RECORD_BTN.with(|b| *b.borrow_mut() = Some(rec.clone()));
        toolbar.append(&rec);
        let pause = widgets::icon_button("media-playback-pause-symbolic", "Pause recording (Ctrl+Shift+P)");
        pause.set_visible(false);
        pause.connect_clicked(|_| pause_recording());
        PAUSE_BTN.with(|b| *b.borrow_mut() = Some(pause.clone()));
        toolbar.append(&pause);
        glib::timeout_add_seconds_local(1, || {
            refresh_record_button();
            glib::ControlFlow::Continue
        });
        refresh_record_button();
    }
    if tools.can_webcam() {
        let cam = widgets::icon_button("camera-web-symbolic", "Picture-in-picture webcam");
        cam.connect_clicked(|_| toggle_webcam());
        CAM_BTN.with(|b| *b.borrow_mut() = Some(cam.clone()));
        toolbar.append(&cam);
        refresh_cam_button();
    }

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.add_css_class("settings-content");
    content.set_hexpand(true);
    content.append(&chrome::search_strip(&search));
    content.append(&toolbar);
    content.append(&hint);
    content.append(&scroller);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    body.append(&nav);
    body.append(&content);

    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("window-frame");
    frame.append(&body);
    frame.append(&status_bar());

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&frame));
    window.set_child(Some(&overlay));
    install_keys(&window, &search);

    let ui = Ui {
        window: window.clone(),
        overlay: overlay.clone(),
        nav: nav.clone(),
        nav_list: list,
        search,
        status: status_label(),
        hint,
        scroller,
        grid,
        view: View::Recent,
        items: Vec::new(),
    };
    UI.with(|slot| *slot.borrow_mut() = Some(Rc::new(RefCell::new(ui))));

    let apply_width = {
        let nav = nav.clone();
        let window = window.clone();
        move || {
            let width = if window.width() > 0 { window.width() } else { window.default_width() };
            let narrow = width > 0 && width < 980;
            settings_dialog::fit(&window);
            if narrow == NARROW.with(|n| n.get()) && nav.has_css_class("sized") {
                return;
            }
            nav.add_css_class("sized");
            NARROW.with(|n| n.set(narrow));
            apply_compact(&nav, narrow || prefs::get().sidebar_collapsed);
        }
    };
    let aw = apply_width.clone();
    window.connect_realize(move |_| aw());
    let probe = gtk::DrawingArea::new();
    probe.set_can_target(false);
    probe.set_can_focus(false);
    overlay.add_overlay(&probe);
    overlay.set_measure_overlay(&probe, false);
    probe.connect_resize(move |_, _, _| {
        let aw = apply_width.clone();
        glib::idle_add_local_once(aw);
    });
    rebuild_nav();
    reload();
    prune_history();
}

pub fn capture_now(mode: CaptureMode) {
    capture_with(mode, after_from_prefs(), 0);
}

pub fn capture_with(mode: CaptureMode, after: After, delay: u32) {
    let win = window();
    if let Some(w) = &win {
        w.minimize();
    }
    let langs = prefs::get().ocr_langs;
    let ocr_mode = OcrMode::parse(&prefs::get().ocr_mode);
    let save_path = save_file();
    let restore = {
        let win = win.clone();
        Rc::new(move || {
            if let Some(w) = &win {
                w.present();
            }
        })
    };
    if delay > 0 {
        let restore_wait = restore.clone();
        let go = {
            let restore = restore.clone();
            let langs = langs.clone();
            let save_path = save_path.clone();
            let mode = mode.clone();
            move || begin_capture(mode, after, langs, ocr_mode, save_path, restore)
        };
        match crate::overlay::countdown(delay, move |ok| {
            if ok {
                go();
            } else {
                restore_wait();
            }
        }) {
            Ok(()) => return,
            Err(err) => toast(&err.to_string()),
        }
    }
    begin_capture(mode, after, langs, ocr_mode, save_path, restore);
}

fn begin_capture(
    mode: CaptureMode,
    after: After,
    langs: String,
    ocr_mode: OcrMode,
    save_path: Option<PathBuf>,
    restore: Rc<dyn Fn()>,
) {
    let backend = prefs::get().backend;
    let use_overlay = matches!(mode, CaptureMode::SelectRegion | CaptureMode::SelectWindow)
        && crate::overlay::prefer()
        && nixe_capture_core::capture::pick(&backend) != "portal";
    if use_overlay {
        let restore = restore.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            start_overlay(mode, after, langs, ocr_mode, save_path, restore);
        });
        return;
    }
    let langs_ui = langs.clone();
    let save_done = save_path.clone();
    crate::cmd::background(
        move || {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let action = Action { mode, after, delay: 0, save_path, ocr_langs: langs, ocr_mode, backend };
            action::capture(&action)
        },
        {
            let restore = restore.clone();
            move |result| {
                restore();
                match result {
                    Ok(Some(shot)) => finish_shot(shot, after, langs_ui, ocr_mode, save_done),
                    Ok(None) => {}
                    Err(err) => report_capture(Err(err)),
                }
            }
        },
    );
}

fn after_from_prefs() -> After {
    match prefs::get().after_capture.as_str() {
        "save" => After::Save,
        "ocr" => After::Ocr,
        "copy" => After::Copy,
        "pin" => After::Pin,
        _ => After::Edit,
    }
}

fn save_file() -> Option<PathBuf> {
    let dir = prefs::get().save_dir;
    if dir.is_empty() {
        None
    } else {
        Some(action::save_path_in(std::path::Path::new(&dir)))
    }
}

fn finish_shot(
    shot: nixe_capture_core::capture::Shot,
    after: After,
    langs: String,
    ocr_mode: OcrMode,
    save_path: Option<PathBuf>,
) {
    match after {
        After::Edit => crate::editor::open(shot),
        After::Pin => {
            remember(&shot);
            crate::pin::open_shot(shot);
        }
        other => {
            remember(&shot);
            crate::cmd::background(
                move || {
                    let action = Action { mode: CaptureMode::FullDesktop, after: other, delay: 0, save_path, ocr_langs: langs, ocr_mode, backend: String::new() };
                    action::finish(&shot, &action)
                },
                report_capture,
            );
        }
    }
}

fn start_overlay(mode: CaptureMode, after: After, langs: String, ocr_mode: OcrMode, save_path: Option<PathBuf>, restore: Rc<dyn Fn()>) {
    let snap = matches!(mode, CaptureMode::SelectWindow);
    if crate::overlay::supported() {
        match crate::overlay::freeze() {
            Ok(still) => {
                let cropped = still.clone();
                let restore_done = restore.clone();
                let langs_done = langs.clone();
                let save_picked = save_path.clone();
                match crate::overlay::pick(still, snap, move |rect| {
                    let Some(rect) = rect else {
                        restore_done();
                        return;
                    };
                    crate::cmd::background(
                        move || cropped.crop_layout(rect),
                        move |result| match result {
                            Ok(shot) => {
                                if !matches!(after, After::Edit | After::Pin) {
                                    restore_done();
                                }
                                finish_shot(shot, after, langs_done.clone(), ocr_mode, save_picked.clone());
                            }
                            Err(err) => {
                                restore_done();
                                report_capture(Err(err));
                            }
                        },
                    );
                }) {
                    Ok(()) => return,
                    Err(err) => toast(&err.to_string()),
                }
            }
            Err(err) => {
                restore();
                toast(&err.to_string());
                return;
            }
        }
    }
    let langs_ui = langs.clone();
    let save_done = save_path.clone();
    crate::cmd::background(
        move || {
            let action = Action { mode, after, delay: 0, save_path, ocr_langs: langs, ocr_mode, backend: String::new() };
            action::capture(&action)
        },
        {
            let restore = restore.clone();
            move |result| {
                restore();
                match result {
                    Ok(Some(shot)) => finish_shot(shot, after, langs_ui, ocr_mode, save_done),
                    Ok(None) => {}
                    Err(err) => report_capture(Err(err)),
                }
            }
        },
    );
}

fn remember(shot: &nixe_capture_core::capture::Shot) {
    if !prefs::get().history {
        return;
    }
    let Ok(png) = shot.png() else { return };
    if png.is_empty() {
        return;
    }
    let (w, h) = (shot.image.width() as i32, shot.image.height() as i32);
    let image = shot.image.clone();
    let ocr_on = prefs::get().ocr_on_capture;
    let langs = prefs::get().ocr_langs;
    let mode = OcrMode::parse(&prefs::get().ocr_mode);
    crate::cmd::background(
        move || {
            let db = store::open(&paths::library_db())?;
            let (id, path) = db.insert_png(&paths::captures_dir(), &png, w, h, "grim")?;
            let _ = store::write_thumb(&path, &paths::thumb_file(&id), store::THUMB_EDGE);
            if ocr_on && let Ok(r) = recognize(&image, mode, &langs) {
                let _ = db.set_ocr(&id, &r.engine, &r.text);
            }
            anyhow::Ok(())
        },
        |result| match result {
            Ok(()) => reload(),
            Err(err) => toast(&err.to_string()),
        },
    );
}

fn prune_history() {
    if !prefs::get().history {
        return;
    }
    let days = prefs::get().retention_days;
    crate::cmd::background(
        move || {
            let db = store::open(&paths::library_db())?;
            db.prune(days)
        },
        |result| match result {
            Ok(gone) if !gone.is_empty() => {
                for p in &gone {
                    if p.extension().is_some_and(|e| e == "png")
                        && let Some(id) = p.file_stem()
                    {
                        crate::grid::forget_thumb(&id.to_string_lossy());
                    }
                }
                reload();
            }
            Err(err) => toast(&err.to_string()),
            _ => {}
        },
    );
}

pub fn toggle_record(region: bool) {
    if record::active().is_some() {
        stop_recording();
        return;
    }
    start_recording(region);
}

pub fn start_recording(region: bool) {
    if record::active().is_some() {
        toast("Already recording");
        return;
    }
    let delay = prefs::get().record_delay;
    if delay > 0 {
        let _ = crate::overlay::countdown(delay, move |ok| {
            if ok {
                begin_record(region);
            }
        });
        return;
    }
    begin_record(region);
}

pub fn stop_recording() {
    crate::cmd::background(record::stop, |result| match result {
        Ok(Some(path)) => {
            remember_recording(&path);
            refresh_record_button();
            refresh_cam_button();
            toast(&format!("Saved {}", path.display()));
        }
        Ok(None) => {
            refresh_record_button();
            refresh_cam_button();
        }
        Err(err) => {
            refresh_record_button();
            refresh_cam_button();
            toast(&err.to_string());
        }
    });
}

fn begin_record(region: bool) {
    if record::pick(&prefs::get().recorder) == Some(nixe_capture_core::RecordBackend::GstPortal) {
        launch_record(Target::Portal);
        return;
    }
    if region && crate::overlay::prefer() && crate::overlay::supported() {
        match crate::overlay::freeze() {
            Ok(still) => {
                if crate::overlay::pick(still, false, move |rect| {
                    let Some(rect) = rect else { return };
                    launch_record(Target::Region(rect));
                })
                .is_err()
                {
                    launch_record(record_target(true));
                }
            }
            Err(err) => toast(&err.to_string()),
        }
        return;
    }
    launch_record(record_target(region));
}

fn record_target(region: bool) -> Target {
    if !region && prefs::get().record_use_portal {
        return Target::Portal;
    }
    if region && let Ok(Some(r)) = nixe_capture_core::capture::slurp_region() {
        return Target::Region(r);
    }
    record::default_target().unwrap_or(Target::Portal)
}

fn record_request(target: Target, output: PathBuf) -> RecordRequest {
    let p = prefs::get();
    let (desktop_audio, microphone) = match p.record_audio.as_str() {
        "mic" => (false, true),
        "both" => (true, true),
        "none" => (false, false),
        _ => (true, false),
    };
    RecordRequest {
        target,
        desktop_audio,
        microphone,
        cursor: p.record_cursor,
        container: Container::parse(&p.record_container),
        fps: 60,
        output,
        backend: p.recorder,
    }
}

fn record_output_path() -> PathBuf {
    let container = Container::parse(&prefs::get().record_container);
    let dir = if prefs::get().history { paths::captures_dir() } else { record::default_output_dir() };
    record::output_path(&dir, container)
}

fn launch_record(target: Target) {
    let req = record_request(target, record_output_path());
    let p = prefs::get();
    let cam = p.record_webcam;
    let cam_device = p.record_webcam_device;
    let cam_size = p.record_webcam_size;
    crate::cmd::background(
        move || start_record_session(&req, cam, &cam_device, &cam_size),
        |result| match result {
            Ok(_) => {
                refresh_record_button();
                refresh_cam_button();
                toast("Recording");
            }
            Err(err) => toast(&err.to_string()),
        },
    );
}

fn start_record_session(
    req: &RecordRequest,
    cam: bool,
    cam_device: &str,
    cam_size: &str,
) -> anyhow::Result<nixe_capture_core::RecordSession> {
    let started_cam = cam && nixe_capture_core::webcam::available() && !nixe_capture_core::webcam::active();
    if started_cam {
        let device = if cam_device.is_empty() { None } else { Some(cam_device) };
        if let Err(err) = nixe_capture_core::webcam::start(device, nixe_capture_core::webcam::Size::parse(cam_size), true) {
            eprintln!("nixe-capture: webcam overlay: {err}");
        }
    }
    match record::start(req) {
        Ok(session) => Ok(session),
        Err(err) => {
            if started_cam {
                nixe_capture_core::webcam::stop();
            }
            Err(err)
        }
    }
}

pub fn pause_recording() {
    crate::cmd::background(record::toggle_pause, |result| match result {
        Ok(true) => {
            refresh_record_button();
            toast("Paused");
        }
        Ok(false) => {
            refresh_record_button();
            toast("Recording");
        }
        Err(err) => toast(&err.to_string()),
    });
}

pub fn toggle_webcam() {
    if nixe_capture_core::webcam::active() {
        nixe_capture_core::webcam::stop();
        refresh_cam_button();
        return;
    }
    let p = prefs::get();
    let device = p.record_webcam_device.clone();
    let size = p.record_webcam_size.clone();
    crate::cmd::background(
        move || {
            let dev = if device.is_empty() { None } else { Some(device.as_str()) };
            nixe_capture_core::webcam::start(dev, nixe_capture_core::webcam::Size::parse(&size), false)
        },
        |result| match result {
            Ok(_) => refresh_cam_button(),
            Err(err) => {
                refresh_cam_button();
                toast(&err.to_string());
            }
        },
    );
}

fn remember_recording(path: &std::path::Path) {
    if !prefs::get().history {
        return;
    }
    let path = path.to_path_buf();
    crate::cmd::background(
        move || {
            let db = store::open(&paths::library_db())?;
            let id = db.insert(&store::NewCapture {
                kind: CaptureKind::Recording,
                width: 0,
                height: 0,
                path: path.clone(),
                source: "record".into(),
            })?;
            let _ = store::write_thumb(&path, &paths::thumb_file(&id), store::THUMB_EDGE);
            anyhow::Ok(())
        },
        |result| match result {
            Ok(()) => reload(),
            Err(err) => toast(&err.to_string()),
        },
    );
}

fn refresh_record_button() {
    RECORD_BTN.with(|b| {
        let Some(btn) = b.borrow().clone() else { return };
        if record::active().is_some() {
            btn.set_icon_name("media-playback-stop-symbolic");
            btn.set_tooltip_text(Some("Stop recording (Ctrl+R)"));
            btn.add_css_class("destructive-action");
        } else {
            btn.set_icon_name("media-record-symbolic");
            btn.set_tooltip_text(Some("Start a recording (Ctrl+R)"));
            btn.remove_css_class("destructive-action");
        }
    });
    PAUSE_BTN.with(|b| {
        let Some(btn) = b.borrow().clone() else { return };
        let session = record::active();
        btn.set_visible(record::can_pause());
        if let Some(session) = session {
            if session.paused {
                btn.set_icon_name("media-playback-start-symbolic");
                btn.set_tooltip_text(Some("Resume recording (Ctrl+Shift+P)"));
            } else {
                btn.set_icon_name("media-playback-pause-symbolic");
                btn.set_tooltip_text(Some("Pause recording (Ctrl+Shift+P)"));
            }
        }
    });
    refresh_cam_button();
    refresh_status();
}

fn refresh_cam_button() {
    CAM_BTN.with(|b| {
        let Some(btn) = b.borrow().clone() else { return };
        if nixe_capture_core::webcam::active() {
            btn.add_css_class("suggested-action");
            btn.set_tooltip_text(Some("Hide webcam overlay"));
        } else {
            btn.remove_css_class("suggested-action");
            btn.set_tooltip_text(Some("Picture-in-picture webcam"));
        }
    });
}

pub fn open_capture(row: CaptureRow) {
    if row.kind == CaptureKind::Recording {
        let path = row.path.to_string_lossy().into_owned();
        crate::cmd::spawn(&["xdg-open", &path]);
        return;
    }
    let path = row.path.clone();
    let id = row.id.clone();
    crate::cmd::background(
        move || {
            let img = image::open(&path)?.to_rgba8();
            let state = annot::EditState::load(&annot::sidecar_path(&path)).unwrap_or_default();
            anyhow::Ok((img, state, id, path))
        },
        |result| match result {
            Ok((img, state, id, path)) => crate::editor::open_saved(img, state, id, path),
            Err(err) => toast(&err.to_string()),
        },
    );
}

pub fn set_starred(id: &str, on: bool) {
    let id = id.to_string();
    crate::cmd::background(
        move || {
            store::open(&paths::library_db())?.set_starred(&id, on)
        },
        |result| match result {
            Ok(()) => reload(),
            Err(err) => toast(&err.to_string()),
        },
    );
}

pub fn item_menu(id: &str, parent: &gtk::Widget, x: f64, y: f64) {
    let Some(ui) = ui() else { return };
    let Some(row) = ui.borrow().items.iter().find(|c| c.id == id).cloned() else { return };
    MENU.with(|m| {
        if let Some(old) = m.borrow_mut().take() {
            old.unparent();
        }
    });
    let pop = gtk::Popover::new();
    pop.set_parent(parent);
    pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    pop.set_has_arrow(false);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("history-menu");
    let open = gtk::Button::with_label("Open");
    open.add_css_class("flat");
    let star = gtk::Button::with_label(if row.starred { "Unstar" } else { "Star" });
    star.add_css_class("flat");
    let pin = gtk::Button::with_label("Pin");
    pin.add_css_class("flat");
    let copy = gtk::Button::with_label("Copy");
    copy.add_css_class("flat");
    let export = gtk::Button::with_label("Export");
    export.add_css_class("flat");
    let delete = gtk::Button::with_label("Delete");
    delete.add_css_class("flat");
    list.append(&open);
    list.append(&star);
    if row.kind != CaptureKind::Recording {
        list.append(&pin);
        list.append(&copy);
    }
    list.append(&export);
    if row.kind == CaptureKind::Recording && nixe_capture_core::detect::present("ffmpeg") {
        let trim = gtk::Button::with_label("Trim");
        trim.add_css_class("flat");
        let pop_trim = pop.clone();
        let row_trim = row.clone();
        trim.connect_clicked(move |_| {
            pop_trim.popdown();
            trim_capture(&row_trim);
        });
        list.append(&trim);
    }
    list.append(&delete);
    pop.set_child(Some(&list));
    let pop_open = pop.clone();
    let row_open = row.clone();
    open.connect_clicked(move |_| {
        pop_open.popdown();
        open_capture(row_open.clone());
    });
    let pop_star = pop.clone();
    let id_star = row.id.clone();
    let star_on = !row.starred;
    star.connect_clicked(move |_| {
        pop_star.popdown();
        set_starred(&id_star, star_on);
    });
    let pop_pin = pop.clone();
    let path_pin = row.path.clone();
    pin.connect_clicked(move |_| {
        pop_pin.popdown();
        pin_file(&path_pin);
    });
    let pop_copy = pop.clone();
    let path_copy = row.path.clone();
    copy.connect_clicked(move |_| {
        pop_copy.popdown();
        copy_capture(&path_copy);
    });
    let pop_export = pop.clone();
    let id_export = row.id.clone();
    let name_export = row.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "capture.png".into());
    export.connect_clicked(move |_| {
        pop_export.popdown();
        export_capture(&id_export, &name_export);
    });
    let pop_del = pop.clone();
    let id_del = row.id.clone();
    delete.connect_clicked(move |_| {
        pop_del.popdown();
        confirm_delete(&id_del);
    });
    MENU.with(|m| *m.borrow_mut() = Some(pop.clone()));
    pop.popup();
}

fn pin_file(path: &std::path::Path) {
    match image::open(path) {
        Ok(img) => crate::pin::open_rgba(img.to_rgba8()),
        Err(err) => toast(&err.to_string()),
    }
}

fn export_capture(id: &str, name: &str) {
    let id = id.to_string();
    let dialog = gtk::FileDialog::builder().title("Export capture").modal(true).initial_name(name).build();
    dialog.save(window().as_ref(), gio::Cancellable::NONE, move |res| {
        let Ok(file) = res else { return };
        let Some(path) = file.path() else { return };
        let id = id.clone();
        crate::cmd::background(
            move || export_id(&id, &path).map(|_| path),
            |result| match result {
                Ok(path) => toast(&format!("Saved {}", path.display())),
                Err(err) => toast(&err.to_string()),
            },
        );
    });
}

fn trim_capture(row: &CaptureRow) {
    let path = row.path.clone();
    let (dialog, card) = widgets::dialog("Trim recording", 420);
    let note = widgets::label("Seconds from the start. Leave the end empty to keep the rest. The original file stays.", "dim");
    note.set_wrap(true);
    card.append(&note);
    let start = gtk::Entry::new();
    start.set_placeholder_text(Some("Start"));
    start.set_text("0");
    card.append(&start);
    let end = gtk::Entry::new();
    end.set_placeholder_text(Some("End (optional)"));
    card.append(&end);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Not now");
    let ok = gtk::Button::with_label("Trim");
    ok.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&ok);
    card.append(&buttons);
    let d = dialog.clone();
    cancel.connect_clicked(move |_| d.close());
    let d = dialog.clone();
    ok.connect_clicked(move |_| {
        let start_s = start.text().to_string();
        let end_s = end.text().to_string();
        let Ok(start_t) = start_s.trim().parse::<f64>() else {
            start.add_css_class("error");
            return;
        };
        if start_t < 0.0 {
            start.add_css_class("error");
            return;
        }
        let end_t = match end_s.trim() {
            "" => None,
            s => match s.parse::<f64>() {
                Ok(n) if n > start_t => Some(n),
                _ => {
                    end.add_css_class("error");
                    return;
                }
            },
        };
        d.close();
        let src = path.clone();
        crate::cmd::background(
            move || {
                let dest = trim_dest(&src);
                record::trim(&src, &dest, start_t, end_t)?;
                anyhow::Ok(dest)
            },
            |result| match result {
                Ok(dest) => {
                    remember_recording(&dest);
                    toast(&format!("Saved {}", dest.display()));
                }
                Err(err) => toast(&err.to_string()),
            },
        );
    });
    dialog.present();
}

fn trim_dest(src: &std::path::Path) -> PathBuf {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("recording");
    let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let dir = src.parent().unwrap_or_else(|| std::path::Path::new("."));
    let dest = dir.join(format!("{stem}-trim.{ext}"));
    if dest.exists() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        dir.join(format!("{stem}-trim-{stamp}.{ext}"))
    } else {
        dest
    }
}

fn copy_capture(path: &std::path::Path) {
    let path = path.to_path_buf();
    crate::cmd::background(
        move || {
            let bytes = std::fs::read(&path)?;
            clipboard::copy_png(&bytes)
        },
        |result| match result {
            Ok(()) => toast("Copied"),
            Err(err) => toast(&err.to_string()),
        },
    );
}

fn confirm_delete(id: &str) {
    let id = id.to_string();
    let (dialog, card) = widgets::dialog("Delete this capture?", 420);
    let note = widgets::label("The file and its notes leave this computer.", "dim");
    note.set_wrap(true);
    card.append(&note);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Not now");
    let del = gtk::Button::with_label("Delete");
    del.add_css_class("destructive-action");
    buttons.append(&cancel);
    buttons.append(&del);
    card.append(&buttons);
    let d = dialog.clone();
    cancel.connect_clicked(move |_| d.close());
    let d = dialog.clone();
    del.connect_clicked(move |_| {
        d.close();
        let id = id.clone();
        let id_ui = id.clone();
        crate::cmd::background(
            move || store::open(&paths::library_db())?.delete(&id),
            move |result| match result {
                Ok(_) => {
                    crate::grid::forget_thumb(&id_ui);
                    reload();
                    toast("Deleted");
                }
                Err(err) => toast(&err.to_string()),
            },
        );
    });
    dialog.present();
}

fn report_capture(result: Result<action::Outcome, anyhow::Error>) {
    match result {
        Ok(action::Outcome::Cancelled) => {}
        Ok(action::Outcome::Copied) => toast("Copied"),
        Ok(action::Outcome::Saved(path)) => toast(&format!("Saved {}", path.display())),
        Ok(action::Outcome::Text(_)) => toast("Copied text"),
        Err(err) => toast(&err.to_string()),
    }
}

fn status_label() -> gtk::Label {
    STATUS.with(|s| s.borrow().clone().unwrap_or_else(|| widgets::label("", "status-readout")))
}

fn status_bar() -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    bar.add_css_class("status-bar");
    bar.append(&chrome::shortcuts_button(show_shortcuts));
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);
    let readout = widgets::label("", "status-readout");
    readout.set_ellipsize(gtk::pango::EllipsizeMode::Start);
    STATUS.with(|s| *s.borrow_mut() = Some(readout.clone()));
    bar.append(&readout);
    bar.append(&chrome::gear(settings_dialog::open));
    bar
}

fn apply_compact(nav: &gtk::Box, compact: bool) {
    if let Some(head) = nav.first_child().and_downcast::<gtk::Box>() {
        chrome::set_head_compact(&head, compact);
    }
    if compact {
        nav.add_css_class("compact");
    } else {
        nav.remove_css_class("compact");
    }
    fn walk(widget: &gtk::Widget, compact: bool) {
        if widget.has_css_class("compact-hide") {
            widget.set_visible(!compact);
        }
        if widget.has_css_class("nav-item")
            && let Some(content) = widget.downcast_ref::<gtk::Button>().and_then(|b| b.child())
        {
            content.set_halign(if compact { gtk::Align::Center } else { gtk::Align::Fill });
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            walk(&next, compact);
            child = next.next_sibling();
        }
    }
    walk(nav.upcast_ref(), compact);
}

pub fn toggle_sidebar() {
    prefs::update(|p| p.sidebar_collapsed = !p.sidebar_collapsed);
    let Some(ui) = ui() else { return };
    let nav = ui.borrow().nav.clone();
    apply_compact(&nav, NARROW.with(|n| n.get()) || prefs::get().sidebar_collapsed);
}

fn snapshot_and_quit(app: &gtk::Application, out: PathBuf) {
    let Some(ui) = ui() else { return };
    let window = ui.borrow().window.clone();
    window.set_opacity(0.01);
    window.set_title(Some("Capture snapshot"));
    window.set_default_size(
        std::env::var("CAPTURE_SNAPSHOT_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1120),
        std::env::var("CAPTURE_SNAPSHOT_H").ok().and_then(|v| v.parse().ok()).unwrap_or(800),
    );
    window.present();
    let app = app.clone();
    let delay: u64 = std::env::var("CAPTURE_SNAPSHOT_DELAY").ok().and_then(|v| v.parse().ok()).unwrap_or(900);
    glib::timeout_add_local_once(std::time::Duration::from_millis(delay), move || {
        if let Some(child) = window.child() {
            let paintable = gtk::WidgetPaintable::new(Some(&child));
            let (w, h) = (child.width(), child.height());
            let snapshot = gtk::Snapshot::new();
            snapshot.append_color(&gdk::RGBA::BLACK, &gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32));
            paintable.snapshot(&snapshot, w as f64, h as f64);
            if let (Some(node), Some(renderer)) = (snapshot.to_node(), window.renderer()) {
                let texture = renderer.render_texture(node, None);
                match texture.save_to_png(&out) {
                    Ok(()) => println!("snapshot {w}x{h} -> {}", out.display()),
                    Err(e) => eprintln!("snapshot failed: {e}"),
                }
            }
        }
        app.quit();
    });
}

fn typing(window: &gtk::Window) -> bool {
    let Some(focus) = gtk::prelude::GtkWindowExt::focus(window) else { return false };
    focus.is::<gtk::Text>() || focus.is::<gtk::TextView>() || focus.ancestor(gtk::Entry::static_type()).is_some()
}

fn install_keys(window: &gtk::ApplicationWindow, search: &gtk::SearchEntry) {
    let keys = gtk::EventControllerKey::new();
    let search = search.clone();
    let win = window.clone();
    keys.connect_key_pressed(move |_, key, _, mods| {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        if ctrl && key == gdk::Key::f {
            search.set_visible(true);
            search.grab_focus();
            return glib::Propagation::Stop;
        }
        if ctrl && shift && key == gdk::Key::l {
            start_scroll();
            return glib::Propagation::Stop;
        }
        if ctrl && shift && key == gdk::Key::n {
            capture_now(CaptureMode::SelectWindow);
            return glib::Propagation::Stop;
        }
        if ctrl && shift && key == gdk::Key::f {
            capture_now(CaptureMode::FullDesktop);
            return glib::Propagation::Stop;
        }
        if ctrl && shift && key == gdk::Key::r {
            toggle_record(true);
            return glib::Propagation::Stop;
        }
        if ctrl && shift && key == gdk::Key::p {
            pause_recording();
            return glib::Propagation::Stop;
        }
        if ctrl && key == gdk::Key::r {
            toggle_record(false);
            return glib::Propagation::Stop;
        }
        if ctrl && key == gdk::Key::n {
            capture_now(CaptureMode::SelectRegion);
            return glib::Propagation::Stop;
        }
        if ctrl && key == gdk::Key::b {
            toggle_sidebar();
            return glib::Propagation::Stop;
        }
        if typing(win.upcast_ref()) {
            return glib::Propagation::Proceed;
        }
        glib::Propagation::Proceed
    });
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    window.add_controller(keys);
}

pub fn show_shortcuts() {
    let (dialog, card) = widgets::dialog("Keyboard shortcuts", 520);
    let list = widgets::vbox(0);
    list.add_css_class("group-list");
    for (keys, what) in SHORTCUTS {
        list.append(&widgets::row(what, "", Some(widgets::key_caps(keys).upcast_ref())));
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .propagate_natural_height(true)
        .max_content_height(560)
        .child(&list)
        .build();
    card.append(&scroll);
    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    let dialog2 = dialog.clone();
    close.connect_clicked(move |_| dialog2.close());
    card.append(&close);
    dialog.present();
}

pub fn toast(message: &str) {
    let Some(ui) = ui() else {
        eprintln!("nixe-capture: {message}");
        return;
    };
    let overlay = ui.borrow().overlay.clone();
    if let Some(old) = TOAST.with(|t| t.borrow_mut().take())
        && old.parent().is_some()
    {
        overlay.remove_overlay(&old);
    }
    let label = gtk::Label::new(Some(message));
    label.set_wrap(true);
    label.set_max_width_chars(70);
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bx.add_css_class("toast");
    bx.set_halign(gtk::Align::Center);
    bx.set_valign(gtk::Align::End);
    bx.set_margin_bottom(48);
    bx.append(&label);
    overlay.add_overlay(&bx);
    TOAST.with(|t| *t.borrow_mut() = Some(bx.clone()));
    let overlay = overlay.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(2200), move || {
        if bx.parent().is_some() {
            overlay.remove_overlay(&bx);
        }
        TOAST.with(|t| *t.borrow_mut() = None);
    });
}
