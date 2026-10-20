//! Fullscreen still overlay: region/window pick, magnifier, dimensions, keys.
//!
//! Layer Shell on Hyprland. slurp if the compositor has no layer-shell.

use crate::prefs;
use gtk::cairo::{self, Context, Format, ImageSurface};
use gtk::gdk::{self, prelude::*};
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use nixe_capture_core::capture::{CaptureBackend, CaptureMode, CaptureRequest, Grim, Shot};
use nixe_capture_core::geom::Rect;
use nixe_capture_core::select;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A click that covers less than this many layout pixels is a snap, not a drag.
const CLICK_AREA: i64 = 20;
const MAG_SIZE: f64 = 128.0;
const MAG_ZOOM: f64 = 4.0;

struct Session {
    still: Shot,
    surface: ImageSurface,
    snap: bool,
    dragging: bool,
    origin: Option<(i32, i32)>,
    current: Option<Rect>,
    hinted: Option<Rect>,
    cursor: (i32, i32),
    candidates: Vec<Rect>,
    canvases: Vec<gtk::DrawingArea>,
    windows: Vec<gtk::Window>,
    on_pick: Option<Box<dyn FnOnce(Option<Rect>)>>,
}

type DelayDone = Box<dyn FnOnce(bool)>;

struct DelayUi {
    window: gtk::Window,
    label: gtk::Label,
    remaining: Cell<u32>,
    tick: RefCell<Option<glib::SourceId>>,
    on_done: RefCell<Option<DelayDone>>,
}

thread_local! {
    static SESSION: RefCell<Option<Rc<RefCell<Session>>>> = const { RefCell::new(None) };
    static THEME: Cell<bool> = const { Cell::new(false) };
    static DISMISSING: Cell<bool> = const { Cell::new(false) };
    static DELAY: RefCell<Option<Rc<DelayUi>>> = const { RefCell::new(None) };
    static DELAY_DISMISSING: Cell<bool> = const { Cell::new(false) };
}

/// `auto` and `overlay` try the GTK selector; `slurp` skips it.
pub fn prefer() -> bool {
    prefs::get().selector != "slurp"
}

pub fn supported() -> bool {
    gtk4_layer_shell::is_supported()
}

pub fn freeze() -> anyhow::Result<Shot> {
    Grim.capture(&CaptureRequest { mode: CaptureMode::FullDesktop })
}

/// GTK + theme for the cold `--region` path, which never opens the history window.
pub fn ensure_gtk() -> anyhow::Result<()> {
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: still single-threaded; nothing else reads the environment yet.
        unsafe { std::env::set_var("GSK_RENDERER", "ngl") };
    }
    gtk::init()?;
    if !THEME.with(|t| t.get()) {
        crate::theme::install();
        THEME.with(|t| t.set(true));
    }
    Ok(())
}

pub fn mark_theme_installed() {
    THEME.with(|t| t.set(true));
}

/// Seconds on a small overlay, then `on_done(true)`. Esc calls `on_done(false)`.
pub fn countdown(secs: u32, on_done: impl FnOnce(bool) + 'static) -> anyhow::Result<()> {
    if secs == 0 {
        on_done(true);
        return Ok(());
    }
    finish_delay(false);
    if !supported() {
        let left = Rc::new(Cell::new(secs));
        let done = Rc::new(RefCell::new(Some(on_done)));
        glib::timeout_add_seconds_local(1, move || {
            let n = left.get().saturating_sub(1);
            left.set(n);
            if n == 0 {
                if let Some(cb) = done.borrow_mut().take() {
                    cb(true);
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        return Ok(());
    }
    let outputs = outputs()?;
    let (monitor, _) = outputs.into_iter().next().ok_or_else(|| anyhow::anyhow!("no monitors"))?;
    let window = gtk::Window::new();
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("nixe-capture-delay"));
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_anchor(Edge::Top, true);
    window.set_margin(Edge::Top, 56);
    let g = monitor.geometry();
    window.set_margin(Edge::Left, ((g.width() - 168) / 2).max(0));
    window.set_exclusive_zone(0);
    window.set_monitor(Some(&monitor));
    window.set_default_size(168, 108);
    window.set_size_request(168, 108);
    window.add_css_class("capture-countdown");
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_deletable(false);
    if let Some(app) = gtk::gio::Application::default().and_then(|a| a.downcast::<gtk::Application>().ok()) {
        window.set_application(Some(&app));
    }

    let label = gtk::Label::new(Some(&secs.to_string()));
    label.add_css_class("capture-countdown-num");
    let hint = gtk::Label::new(Some("Esc cancels"));
    hint.add_css_class("dim");
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("capture-countdown-card");
    card.set_halign(gtk::Align::Center);
    card.set_valign(gtk::Align::Center);
    card.append(&label);
    card.append(&hint);
    window.set_child(Some(&card));

    let ui = Rc::new(DelayUi {
        window: window.clone(),
        label,
        remaining: Cell::new(secs),
        tick: RefCell::new(None),
        on_done: RefCell::new(Some(Box::new(on_done))),
    });
    DELAY.with(|d| *d.borrow_mut() = Some(ui.clone()));

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            finish_delay(false);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
    window.connect_close_request(|_| {
        if !DELAY_DISMISSING.get() {
            finish_delay(false);
        }
        glib::Propagation::Proceed
    });

    let tick_ui = ui.clone();
    let id = glib::timeout_add_seconds_local(1, move || {
        let n = tick_ui.remaining.get().saturating_sub(1);
        tick_ui.remaining.set(n);
        if n == 0 {
            finish_delay(true);
            glib::ControlFlow::Break
        } else {
            tick_ui.label.set_text(&n.to_string());
            glib::ControlFlow::Continue
        }
    });
    ui.tick.replace(Some(id));
    window.present();
    Ok(())
}

/// CLI wait. Sleeps when layer-shell is missing. False means the user cancelled.
pub fn run_countdown_blocking(secs: u32) -> bool {
    if secs == 0 {
        return true;
    }
    if ensure_gtk().is_err() || !supported() {
        std::thread::sleep(std::time::Duration::from_secs(secs as u64));
        return true;
    }
    let ok = Rc::new(Cell::new(true));
    let main = glib::MainLoop::new(None, false);
    let ok2 = ok.clone();
    let main2 = main.clone();
    match countdown(secs, move |proceed| {
        ok2.set(proceed);
        main2.quit();
    }) {
        Ok(()) => {
            main.run();
            ok.get()
        }
        Err(_) => {
            std::thread::sleep(std::time::Duration::from_secs(secs as u64));
            true
        }
    }
}

fn finish_delay(proceed: bool) {
    if DELAY_DISMISSING.get() {
        return;
    }
    let Some(ui) = DELAY.with(|d| d.borrow_mut().take()) else {
        return;
    };
    DELAY_DISMISSING.set(true);
    if let Some(id) = ui.tick.borrow_mut().take() {
        id.remove();
    }
    let cb = ui.on_done.borrow_mut().take();
    dismiss_windows(vec![ui.window.clone()]);
    glib::idle_add_local_once(move || {
        DELAY_DISMISSING.set(false);
        if let Some(cb) = cb {
            cb(proceed);
        }
    });
}

/// Show the overlay and invoke `on_pick` with a layout rect, or `None` on cancel.
pub fn pick(still: Shot, snap: bool, on_pick: impl FnOnce(Option<Rect>) + 'static) -> anyhow::Result<()> {
    if SESSION.with(|s| s.borrow().is_some()) {
        finish(None);
    }
    let outputs = outputs()?;
    if outputs.is_empty() {
        anyhow::bail!("no monitors");
    }
    let surface = still_surface(&still).ok_or_else(|| anyhow::anyhow!("the freeze could not be drawn"))?;
    let mut candidates = Vec::new();
    if let Ok(list) = select::monitors() {
        candidates.extend(list.into_iter().map(|m| m.geometry));
    }
    if let Ok(list) = select::windows() {
        candidates.extend(list.into_iter().filter(|w| !w.hidden).map(|w| w.geometry));
    }
    if candidates.is_empty() {
        candidates.extend(outputs.iter().map(|(_, r)| *r));
    }
    let session = Rc::new(RefCell::new(Session {
        still,
        surface,
        snap,
        dragging: false,
        origin: None,
        current: None,
        hinted: None,
        cursor: (0, 0),
        candidates,
        canvases: Vec::new(),
        windows: Vec::new(),
        on_pick: Some(Box::new(on_pick)),
    }));
    SESSION.with(|s| *s.borrow_mut() = Some(session.clone()));

    for (monitor, layout) in outputs {
        let window = gtk::Window::new();
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some("nixe-capture"));
        window.set_keyboard_mode(KeyboardMode::Exclusive);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_exclusive_zone(-1);
        window.set_monitor(Some(&monitor));
        let g = monitor.geometry();
        window.set_default_size(g.width(), g.height());
        window.set_size_request(g.width(), g.height());
        window.add_css_class("capture-overlay");
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_deletable(false);
        if let Some(app) = gtk::gio::Application::default().and_then(|a| a.downcast::<gtk::Application>().ok()) {
            window.set_application(Some(&app));
        }

        let canvas = gtk::DrawingArea::new();
        canvas.set_hexpand(true);
        canvas.set_vexpand(true);
        canvas.set_cursor_from_name(Some("crosshair"));
        canvas.set_draw_func({
            let session = session.clone();
            move |_, cr, w, h| {
                if let Ok(s) = session.try_borrow() {
                    draw(&s, cr, w as f64, h as f64, layout);
                }
            }
        });
        wire_pointer(&canvas, session.clone(), layout);
        wire_keys(&window, session.clone());
        window.set_child(Some(&canvas));
        // Proceed so close() actually unmaps. Stop left Exclusive overlay layers
        // mapped on every workspace after a pick.
        window.connect_close_request(|_| {
            if !DISMISSING.get() {
                finish(None);
            }
            glib::Propagation::Proceed
        });
        session.borrow_mut().canvases.push(canvas);
        session.borrow_mut().windows.push(window.clone());
        window.present();
    }
    Ok(())
}

/// Cold CLI: freeze, pick, crop, then copy/save/OCR. Caller has already delayed.
pub fn run_cli(action: &nixe_capture_core::action::Action) -> anyhow::Result<nixe_capture_core::action::Outcome> {
    ensure_gtk()?;
    if !supported() {
        anyhow::bail!("layer-shell isn't available");
    }
    nixe_capture_core::action::require_tools_for(&CaptureMode::FullDesktop, action.after, action.ocr_mode)?;
    let still = freeze()?;
    let snap = matches!(action.mode, CaptureMode::SelectWindow);
    let picked = run_blocking(still.clone(), snap);
    match picked {
        None => Ok(nixe_capture_core::action::Outcome::Cancelled),
        Some(rect) => {
            let shot = still.crop_layout(rect)?;
            nixe_capture_core::action::finish(&shot, action)
        }
    }
}

fn run_blocking(still: Shot, snap: bool) -> Option<Rect> {
    let result = Rc::new(RefCell::new(None::<Option<Rect>>));
    let main = glib::MainLoop::new(None, false);
    let result2 = result.clone();
    let main2 = main.clone();
    if pick(still, snap, move |rect| {
        *result2.borrow_mut() = Some(rect);
        main2.quit();
    })
    .is_err()
    {
        return None;
    }
    main.run();
    result.borrow_mut().take().flatten()
}

fn outputs() -> anyhow::Result<Vec<(gdk::Monitor, Rect)>> {
    let display = gdk::Display::default().ok_or_else(|| anyhow::anyhow!("no display"))?;
    let list = display.monitors();
    let hypr = select::monitors().unwrap_or_default();
    let mut out = Vec::new();
    for i in 0..list.n_items() {
        let Some(mon) = list.item(i).and_downcast::<gdk::Monitor>() else { continue };
        let layout = match_hypr(&mon, &hypr).unwrap_or_else(|| gdk_rect(&mon));
        out.push((mon, layout));
    }
    Ok(out)
}

fn gdk_rect(mon: &gdk::Monitor) -> Rect {
    let g = mon.geometry();
    Rect::new(g.x(), g.y(), g.width(), g.height())
}

fn match_hypr(mon: &gdk::Monitor, hypr: &[select::Monitor]) -> Option<Rect> {
    if let Some(name) = mon.connector()
        && let Some(m) = hypr.iter().find(|m| m.name == name.as_str())
    {
        return Some(m.geometry);
    }
    let g = mon.geometry();
    hypr.iter().find(|m| m.geometry.x == g.x() && m.geometry.y == g.y()).map(|m| m.geometry)
}

fn wire_pointer(canvas: &gtk::DrawingArea, session: Rc<RefCell<Session>>, layout: Rect) {
    let motion = gtk::EventControllerMotion::new();
    let s = session.clone();
    let c = canvas.clone();
    motion.connect_motion(move |_, x, y| {
        let pt = layout_point(layout, c.width() as f64, c.height() as f64, x, y);
        let mut sess = s.borrow_mut();
        sess.cursor = pt;
        sess.hinted = select::snap_rect_at(pt, &sess.candidates);
        if sess.dragging
            && let Some(origin) = sess.origin
        {
            sess.current = Some(drag_rect(origin, pt));
        } else if sess.snap {
            sess.current = sess.hinted;
        }
        drop(sess);
        redraw();
    });
    canvas.add_controller(motion);

    let drag = gtk::GestureDrag::new();
    drag.set_button(gdk::BUTTON_PRIMARY);
    let s = session.clone();
    let c = canvas.clone();
    drag.connect_drag_begin(move |_, x, y| {
        let pt = layout_point(layout, c.width() as f64, c.height() as f64, x, y);
        let mut sess = s.borrow_mut();
        sess.dragging = true;
        sess.origin = Some(pt);
        sess.cursor = pt;
        sess.current = Some(drag_rect(pt, pt));
        sess.hinted = select::snap_rect_at(pt, &sess.candidates);
        drop(sess);
        redraw();
    });
    let s = session.clone();
    let c = canvas.clone();
    drag.connect_drag_update(move |g, dx, dy| {
        let Some((x, y)) = g.start_point() else { return };
        let pt = layout_point(layout, c.width() as f64, c.height() as f64, x + dx, y + dy);
        let mut sess = s.borrow_mut();
        sess.cursor = pt;
        if let Some(origin) = sess.origin {
            sess.current = Some(drag_rect(origin, pt));
        }
        drop(sess);
        redraw();
    });
    let s = session.clone();
    drag.connect_drag_end(move |_, _, _| {
        let picked = {
            let mut sess = s.borrow_mut();
            sess.dragging = false;
            resolve_release(&sess)
        };
        if let Some(rect) = picked {
            finish(Some(rect));
        } else {
            s.borrow_mut().current = None;
            redraw();
        }
    });
    canvas.add_controller(drag);
}

fn wire_keys(window: &gtk::Window, session: Rc<RefCell<Session>>) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(move |_, key, _, mods| {
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        match key {
            gdk::Key::Escape => {
                finish(None);
                return glib::Propagation::Stop;
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                let picked = {
                    let sess = session.borrow();
                    sess.current.or(sess.hinted)
                };
                if let Some(rect) = picked.filter(|r| !r.is_empty()) {
                    finish(Some(rect));
                }
                return glib::Propagation::Stop;
            }
            gdk::Key::space => {
                let mut sess = session.borrow_mut();
                sess.snap = !sess.snap;
                if sess.snap && sess.current.is_none() {
                    sess.current = sess.hinted;
                }
                drop(sess);
                redraw();
                return glib::Propagation::Stop;
            }
            gdk::Key::Left | gdk::Key::Right | gdk::Key::Up | gdk::Key::Down => {
                let (dx, dy) = match key {
                    gdk::Key::Left => (-1, 0),
                    gdk::Key::Right => (1, 0),
                    gdk::Key::Up => (0, -1),
                    gdk::Key::Down => (0, 1),
                    _ => (0, 0),
                };
                let mut sess = session.borrow_mut();
                if sess.current.is_none() {
                    sess.current = sess.hinted;
                }
                if let Some(r) = sess.current {
                    let bounds = sess.still.area;
                    sess.current = Some(if shift { resize_rect(r, dx, dy, bounds) } else { nudge_rect(r, dx, dy, bounds) });
                }
                drop(sess);
                redraw();
                return glib::Propagation::Stop;
            }
            _ => {}
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
}

fn resolve_release(sess: &Session) -> Option<Rect> {
    let current = sess.current?;
    if is_click(current) {
        return sess.hinted.filter(|r| !r.is_empty());
    }
    if current.is_empty() { None } else { Some(current) }
}

fn finish(picked: Option<Rect>) {
    if DISMISSING.get() {
        return;
    }
    DISMISSING.set(true);
    let Some(sess) = SESSION.with(|s| s.borrow_mut().take()) else {
        DISMISSING.set(false);
        return;
    };
    let mut sess = sess.borrow_mut();
    let windows = std::mem::take(&mut sess.windows);
    let canvases = std::mem::take(&mut sess.canvases);
    let cb = sess.on_pick.take();
    drop(sess);
    drop(canvases);
    dismiss_windows(windows);
    glib::idle_add_local_once(move || {
        DISMISSING.set(false);
        if let Some(cb) = cb {
            cb(picked);
        }
    });
}

fn dismiss_windows(windows: Vec<gtk::Window>) {
    for w in windows {
        w.set_keyboard_mode(KeyboardMode::None);
        w.set_can_target(false);
        w.set_child(None::<&gtk::Widget>);
        w.set_application(None::<&gtk::Application>);
        w.set_visible(false);
        w.close();
    }
}

fn redraw() {
    SESSION.with(|s| {
        if let Some(sess) = s.borrow().as_ref() {
            for c in &sess.borrow().canvases {
                c.queue_draw();
            }
        }
    });
}

fn draw(sess: &Session, cr: &Context, ww: f64, wh: f64, layout: Rect) {
    if ww <= 0.0 || wh <= 0.0 || layout.w <= 0 || layout.h <= 0 {
        return;
    }
    paint_still(cr, &sess.surface, &sess.still, layout, ww, wh);

    let veil = token("veil").or_else(|| token("theme_bg"));
    if let Some(c) = veil {
        set_rgba(cr, c, None);
        let _ = cr.paint();
    }

    let shown = sess.current.filter(|r| !is_click(*r)).or(sess.hinted);
    if let Some(r) = shown
        && let Some(part) = intersection(r, layout)
    {
        let (x, y, w, h) = widget_rect(layout, ww, wh, part);
        cr.rectangle(x, y, w, h);
        cr.clip();
        paint_still(cr, &sess.surface, &sess.still, layout, ww, wh);
        cr.reset_clip();
        if let Some(fill) = token("nav_active") {
            set_rgba(cr, fill, Some(0.35));
            cr.rectangle(x, y, w, h);
            let _ = cr.fill();
        }
        if let Some(stroke) = token("theme_accent") {
            set_rgba(cr, stroke, None);
            cr.set_line_width(1.0);
            cr.rectangle(x + 0.5, y + 0.5, (w - 1.0).max(0.0), (h - 1.0).max(0.0));
            let _ = cr.stroke();
        }
        if sess.current.filter(|c| !is_click(*c)).is_some() {
            paint_chip(cr, r, x, y, w, ww, wh);
        }
    }

    if layout.contains(sess.cursor.0, sess.cursor.1) {
        paint_magnifier(sess, cr, layout, ww, wh);
    }
}

fn paint_still(cr: &Context, surface: &ImageSurface, still: &Shot, mon: Rect, ww: f64, wh: f64) {
    let scale = still.scale.max(0.0001);
    let sx = ww / mon.w as f64;
    let sy = wh / mon.h as f64;
    cr.save().ok();
    cr.scale(sx / scale, sy / scale);
    let ox = (mon.x - still.area.x) as f64 * scale;
    let oy = (mon.y - still.area.y) as f64 * scale;
    if cr.set_source_surface(surface, -ox, -oy).is_ok() {
        cr.source().set_filter(cairo::Filter::Nearest);
        let _ = cr.paint();
    }
    cr.restore().ok();
}

fn paint_chip(cr: &Context, r: Rect, x: f64, y: f64, w: f64, ww: f64, wh: f64) {
    let label = format!("{}×{}  {},{}", r.w, r.h, r.x, r.y);
    cr.select_font_face("JetBrains Mono", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(12.0);
    let ext = cr.text_extents(&label).unwrap_or_else(|_| cairo::TextExtents::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
    let pad_x = 8.0;
    let pad_y = 4.0;
    let cw = ext.width() + pad_x * 2.0;
    let ch = ext.height() + pad_y * 2.0;
    let mut cx = x + (w - cw) / 2.0;
    let mut cy = y - ch - 8.0;
    if cy < 8.0 {
        cy = y + 8.0;
    }
    cx = cx.clamp(8.0, (ww - cw - 8.0).max(8.0));
    cy = cy.clamp(8.0, (wh - ch - 8.0).max(8.0));
    if let Some(bg) = token("theme_bg") {
        set_rgba(cr, bg, None);
        cr.rectangle(cx, cy, cw, ch);
        let _ = cr.fill();
    }
    if let Some(border) = token("theme_border") {
        set_rgba(cr, border, None);
        cr.set_line_width(1.0);
        cr.rectangle(cx + 0.5, cy + 0.5, cw - 1.0, ch - 1.0);
        let _ = cr.stroke();
    }
    if let Some(fg) = token("theme_text") {
        set_rgba(cr, fg, None);
        cr.move_to(cx + pad_x - ext.x_bearing(), cy + pad_y - ext.y_bearing());
        let _ = cr.show_text(&label);
    }
}

fn paint_magnifier(sess: &Session, cr: &Context, layout: Rect, ww: f64, wh: f64) {
    let (wx, wy) = widget_point(layout, ww, wh, sess.cursor);
    let mut mx = wx + 24.0;
    let mut my = wy + 24.0;
    if mx + MAG_SIZE > ww - 8.0 {
        mx = wx - MAG_SIZE - 24.0;
    }
    if my + MAG_SIZE > wh - 8.0 {
        my = wy - MAG_SIZE - 24.0;
    }
    mx = mx.clamp(8.0, (ww - MAG_SIZE - 8.0).max(8.0));
    my = my.clamp(8.0, (wh - MAG_SIZE - 8.0).max(8.0));

    cr.save().ok();
    cr.rectangle(mx, my, MAG_SIZE, MAG_SIZE);
    cr.clip();
    let scale = sess.still.scale.max(0.0001);
    let img_x = (sess.cursor.0 - sess.still.area.x) as f64 * scale;
    let img_y = (sess.cursor.1 - sess.still.area.y) as f64 * scale;
    cr.translate(mx + MAG_SIZE / 2.0, my + MAG_SIZE / 2.0);
    cr.scale(MAG_ZOOM, MAG_ZOOM);
    if cr.set_source_surface(&sess.surface, -img_x, -img_y).is_ok() {
        cr.source().set_filter(cairo::Filter::Nearest);
        let _ = cr.paint();
    }
    cr.restore().ok();

    if let Some(stroke) = token("theme_accent") {
        set_rgba(cr, stroke, None);
        cr.set_line_width(1.0);
        cr.rectangle(mx + 0.5, my + 0.5, MAG_SIZE - 1.0, MAG_SIZE - 1.0);
        let _ = cr.stroke();
        let cx = mx + MAG_SIZE / 2.0;
        let cy = my + MAG_SIZE / 2.0;
        cr.move_to(cx, my + 8.0);
        cr.line_to(cx, my + MAG_SIZE - 8.0);
        cr.move_to(mx + 8.0, cy);
        cr.line_to(mx + MAG_SIZE - 8.0, cy);
        let _ = cr.stroke();
    }
}

fn token(name: &str) -> Option<gdk::RGBA> {
    let p = crate::theme::palette();
    match name {
        "theme_accent" => parse_hex(&p.accent),
        "theme_bg" => parse_hex(&p.bg),
        "theme_text" => parse_hex(&p.text),
        "theme_border" => parse_hex(&p.border),
        "nav_active" => Some(mix(parse_hex(&p.bg)?, parse_hex(&p.accent)?, 0.18)),
        "veil" => {
            let mut c = mix(parse_hex(&p.bg)?, parse_hex(&p.shadow)?, 0.45);
            c.set_alpha(0.82);
            Some(c)
        }
        _ => None,
    }
}

fn parse_hex(hex: &str) -> Option<gdk::RGBA> {
    gdk::RGBA::parse(hex).ok()
}

fn mix(a: gdk::RGBA, b: gdk::RGBA, t: f32) -> gdk::RGBA {
    gdk::RGBA::new(
        a.red() + (b.red() - a.red()) * t,
        a.green() + (b.green() - a.green()) * t,
        a.blue() + (b.blue() - a.blue()) * t,
        a.alpha() + (b.alpha() - a.alpha()) * t,
    )
}

fn set_rgba(cr: &Context, c: gdk::RGBA, alpha: Option<f64>) {
    cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, alpha.unwrap_or(c.alpha() as f64));
}

fn still_surface(still: &Shot) -> Option<ImageSurface> {
    let rgba = still.image.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut surface = ImageSurface::create(Format::ARgb32, w as i32, h as i32).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().ok()?;
        for (y, row) in rgba.rows().enumerate() {
            for (x, p) in row.enumerate() {
                data[y * stride + x * 4..][..4].copy_from_slice(&premultiply(p.0).to_ne_bytes());
            }
        }
    }
    Some(surface)
}

fn premultiply(rgba: [u8; 4]) -> u32 {
    let a = rgba[3] as u32;
    let r = (rgba[0] as u32 * a + 127) / 255;
    let g = (rgba[1] as u32 * a + 127) / 255;
    let b = (rgba[2] as u32 * a + 127) / 255;
    (a << 24) | (r << 16) | (g << 8) | b
}

fn layout_point(mon: Rect, ww: f64, wh: f64, x: f64, y: f64) -> (i32, i32) {
    if ww <= 0.0 || wh <= 0.0 {
        return (mon.x, mon.y);
    }
    let lx = mon.x as f64 + x * mon.w as f64 / ww;
    let ly = mon.y as f64 + y * mon.h as f64 / wh;
    (lx.round() as i32, ly.round() as i32)
}

fn widget_point(mon: Rect, ww: f64, wh: f64, pt: (i32, i32)) -> (f64, f64) {
    let sx = ww / mon.w as f64;
    let sy = wh / mon.h as f64;
    ((pt.0 - mon.x) as f64 * sx, (pt.1 - mon.y) as f64 * sy)
}

fn widget_rect(mon: Rect, ww: f64, wh: f64, r: Rect) -> (f64, f64, f64, f64) {
    let sx = ww / mon.w as f64;
    let sy = wh / mon.h as f64;
    ((r.x - mon.x) as f64 * sx, (r.y - mon.y) as f64 * sy, r.w as f64 * sx, r.h as f64 * sy)
}

pub fn drag_rect(a: (i32, i32), b: (i32, i32)) -> Rect {
    let x = a.0.min(b.0);
    let y = a.1.min(b.1);
    Rect::new(x, y, (a.0 - b.0).abs().max(1), (a.1 - b.1).abs().max(1))
}

pub fn is_click(rect: Rect) -> bool {
    rect.area() < CLICK_AREA
}

fn intersection(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    if x1 > x0 && y1 > y0 { Some(Rect::new(x0, y0, x1 - x0, y1 - y0)) } else { None }
}

fn nudge_rect(r: Rect, dx: i32, dy: i32, bounds: Rect) -> Rect {
    clamp_rect(Rect::new(r.x + dx, r.y + dy, r.w, r.h), bounds)
}

fn resize_rect(r: Rect, dw: i32, dh: i32, bounds: Rect) -> Rect {
    clamp_rect(Rect::new(r.x, r.y, r.w + dw, r.h + dh), bounds)
}

fn clamp_rect(r: Rect, bounds: Rect) -> Rect {
    let w = r.w.clamp(1, bounds.w.max(1));
    let h = r.h.clamp(1, bounds.h.max(1));
    let x = r.x.clamp(bounds.x, bounds.x + bounds.w - w);
    let y = r.y.clamp(bounds.y, bounds.y + bounds.h - h);
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_normalizes_corners() {
        assert_eq!(drag_rect((10, 20), (40, 50)), Rect::new(10, 20, 30, 30));
        assert_eq!(drag_rect((40, 50), (10, 20)), Rect::new(10, 20, 30, 30));
    }

    #[test]
    fn tiny_drag_is_a_click() {
        assert!(is_click(Rect::new(0, 0, 3, 4)));
        assert!(!is_click(Rect::new(0, 0, 20, 20)));
    }

    #[test]
    fn arrows_nudge_and_resize() {
        let bounds = Rect::new(0, 0, 100, 80);
        let r = Rect::new(10, 10, 20, 15);
        assert_eq!(nudge_rect(r, 1, -1, bounds), Rect::new(11, 9, 20, 15));
        assert_eq!(resize_rect(r, 2, -3, bounds), Rect::new(10, 10, 22, 12));
        assert_eq!(nudge_rect(Rect::new(0, 0, 10, 10), -5, -5, bounds), Rect::new(0, 0, 10, 10));
    }

    #[test]
    fn layout_maps_widget_origin() {
        let mon = Rect::new(100, 50, 200, 100);
        assert_eq!(layout_point(mon, 200.0, 100.0, 0.0, 0.0), (100, 50));
        assert_eq!(layout_point(mon, 200.0, 100.0, 200.0, 100.0), (300, 150));
    }
}
