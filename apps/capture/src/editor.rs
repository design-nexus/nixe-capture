//! Annotation editor: marks on the still, copy flattened PNG, original file unchanged.

use crate::paths;
use crate::prefs;
use crate::widgets;
use gtk::cairo;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use image::RgbaImage;
use nixe_capture_core::annot::{self, Colour, EditState, History, Mark, NormRect};
use nixe_capture_core::capture::Shot;
use nixe_capture_core::export;
use nixe_capture_core::ocr::{OcrMode, recognize};
use nixe_capture_core::store;
use nixe_capture_core::clipboard;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const WORK_EDGE: u32 = 2048;
const PAD: f64 = 16.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Arrow,
    Line,
    Rect,
    Ellipse,
    Pen,
    Highlighter,
    Text,
    Callout,
    Blur,
    Pixelate,
    Crop,
}

const TOOLS: &[(Tool, &str, &str)] = &[
    (Tool::Arrow, "capture-tool-arrow-symbolic", "Arrow"),
    (Tool::Line, "capture-tool-line-symbolic", "Line"),
    (Tool::Rect, "capture-tool-rect-symbolic", "Rectangle"),
    (Tool::Ellipse, "capture-tool-ellipse-symbolic", "Ellipse"),
    (Tool::Pen, "document-edit-symbolic", "Freehand"),
    (Tool::Highlighter, "capture-tool-highlighter-symbolic", "Highlighter"),
    (Tool::Text, "insert-text-symbolic", "Text"),
    (Tool::Callout, "capture-tool-callout-symbolic", "Numbered marker"),
    (Tool::Blur, "view-conceal-symbolic", "Blur"),
    (Tool::Pixelate, "view-grid-symbolic", "Pixelate"),
    (Tool::Crop, "zoom-fit-best-symbolic", "Crop"),
];

enum Draft {
    Stroke(Vec<(f64, f64)>),
    Shape { from: (f64, f64), to: (f64, f64) },
}

struct Session {
    original: RgbaImage,
    preview_src: RgbaImage,
    work: RgbaImage,
    history: History,
    tool: Tool,
    colour: Colour,
    size: f64,
    draft: Option<Draft>,
    image_rect: (f64, f64, f64, f64),
    path: Option<PathBuf>,
    id: Option<String>,
}

struct Inner {
    window: gtk::ApplicationWindow,
    canvas: gtk::DrawingArea,
    session: RefCell<Option<Session>>,
    tool_buttons: Vec<(Tool, gtk::ToggleButton)>,
    toast_host: gtk::Overlay,
}

#[derive(Clone)]
struct Editor(Rc<Inner>);

pub fn open(shot: Shot) {
    let Some(app) = gio::Application::default().and_then(|a| a.downcast::<gtk::Application>().ok()) else {
        return;
    };
    Editor::show(&app).load_shot(shot);
}

pub fn open_saved(img: RgbaImage, state: annot::EditState, id: String, path: PathBuf) {
    let Some(app) = gio::Application::default().and_then(|a| a.downcast::<gtk::Application>().ok()) else {
        return;
    };
    Editor::show(&app).load_saved(img, state, id, path);
}

impl Editor {
    fn show(app: &gtk::Application) -> Editor {
        let window = gtk::ApplicationWindow::builder().application(app).title("Capture").default_width(960).default_height(720).build();
        window.add_css_class("capture-window");
        window.add_css_class("capture-editor");
        window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));

        let canvas = gtk::DrawingArea::new();
        canvas.set_hexpand(true);
        canvas.set_vexpand(true);
        canvas.set_cursor_from_name(Some("crosshair"));

        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        tools.add_css_class("editor-strip");
        tools.set_hexpand(true);
        let mut tool_buttons = Vec::new();
        let mut first: Option<gtk::ToggleButton> = None;
        for (tool, icon, tip) in TOOLS {
            let b = gtk::ToggleButton::new();
            b.set_icon_name(icon);
            b.set_tooltip_text(Some(tip));
            b.add_css_class("flat");
            b.add_css_class("icon-button");
            b.add_css_class("editor-tool");
            match &first {
                Some(f) => b.set_group(Some(f)),
                None => first = Some(b.clone()),
            }
            tools.append(&b);
            tool_buttons.push((*tool, b));
        }
        tool_buttons[0].1.set_active(true);

        let swatches = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        for c in theme_swatches() {
            let b = gtk::Button::new();
            b.add_css_class("swatch-button");
            b.set_tooltip_text(Some("Colour"));
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.add_css_class("swatch-dot");
            widgets::paint(&dot, &c.css());
            b.set_child(Some(&dot));
            swatches.append(&b);
            let ed_colour = c;
            // wired after Editor exists
            b.set_widget_name(&c.css());
            let _ = ed_colour;
        }

        let size = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 20.0, 1.0);
        size.set_value(6.0);
        size.set_draw_value(false);
        size.set_size_request(88, -1);
        size.set_tooltip_text(Some("Size"));

        let undo = widgets::icon_button("edit-undo-symbolic", "Undo (Ctrl+Z)");
        let redo = widgets::icon_button("edit-redo-symbolic", "Redo (Ctrl+Shift+Z)");
        let copy = widgets::icon_button("edit-copy-symbolic", "Copy annotated image (Ctrl+C)");
        copy.add_css_class("suggested-action");
        let save = widgets::icon_button("document-save-symbolic", "Save annotated image (Ctrl+S)");
        let pin = widgets::icon_button("view-pin-symbolic", "Pin on the desktop");
        let ocr = widgets::icon_button("insert-text-symbolic", "Read text in the shot");
        let redact = widgets::icon_button("channel-secure-symbolic", "Redact emails, URLs, cards, and keys");
        let close = widgets::icon_button("window-close-symbolic", "Close (Esc)");

        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        strip.add_css_class("editor-bar");
        strip.append(&tools);
        strip.append(&swatches);
        strip.append(&size);
        strip.append(&undo);
        strip.append(&redo);
        strip.append(&copy);
        strip.append(&save);
        strip.append(&pin);
        strip.append(&ocr);
        strip.append(&redact);
        strip.append(&close);

        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.add_css_class("window-frame");
        frame.append(&canvas);
        frame.append(&strip);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&frame));
        window.set_child(Some(&overlay));

        let editor = Editor(Rc::new(Inner {
            window: window.clone(),
            canvas: canvas.clone(),
            session: RefCell::new(None),
            tool_buttons,
            toast_host: overlay,
        }));

        let me = editor.clone();
        canvas.set_draw_func(move |_, cr, w, h| me.draw(cr, w as f64, h as f64));
        editor.wire_pointer();
        editor.wire_keys();

        for (tool, btn) in &editor.0.tool_buttons {
            let me = editor.clone();
            let tool = *tool;
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    me.set_tool(tool);
                }
            });
        }
        // Swatch buttons are children of `swatches`; connect by walking strip.
        let me = editor.clone();
        walk_swatches(&swatches, move |c| me.set_colour(c));

        let me = editor.clone();
        size.connect_value_changed(move |s| {
            if let Some(sess) = me.0.session.borrow_mut().as_mut() {
                sess.size = s.value();
            }
        });
        let me = editor.clone();
        undo.connect_clicked(move |_| me.undo());
        let me = editor.clone();
        redo.connect_clicked(move |_| me.redo());
        let me = editor.clone();
        copy.connect_clicked(move |_| me.copy_flat());
        let me = editor.clone();
        save.connect_clicked(move |_| me.save_flat());
        let me = editor.clone();
        pin.connect_clicked(move |_| me.pin());
        let me = editor.clone();
        ocr.connect_clicked(move |_| me.ocr());
        let me = editor.clone();
        redact.connect_clicked(move |_| me.redact());
        let win = window.clone();
        close.connect_clicked(move |_| win.close());

        window.present();
        editor
    }

    fn load_shot(&self, shot: Shot) {
        let original = shot.image.to_rgba8();
        let png = shot.png().unwrap_or_default();
        let preview_src = annot::preview(&original, WORK_EDGE);
        let work = annot::render(&preview_src, &EditState::default());
        let (w, h) = (original.width() as i32, original.height() as i32);
        let persist = prefs::get().history;
        self.0.session.replace(Some(Session {
            original,
            preview_src,
            work,
            history: History::default(),
            tool: Tool::Arrow,
            colour: theme_swatches().into_iter().nth(2).unwrap_or(Colour::rgb(0.4, 0.6, 1.0)),
            size: 6.0,
            draft: None,
            image_rect: (0.0, 0.0, 1.0, 1.0),
            path: None,
            id: None,
        }));
        if persist && !png.is_empty() {
            let me = self.clone();
            crate::cmd::background(
                move || {
                    let db = store::open(&paths::library_db())?;
                    let (id, path) = db.insert_png(&paths::captures_dir(), &png, w, h, "grim")?;
                    let _ = store::write_thumb(&path, &paths::thumb_file(&id), store::THUMB_EDGE);
                    anyhow::Ok((id, path))
                },
                move |result| match result {
                    Ok((id, path)) => {
                        if let Some(s) = me.0.session.borrow_mut().as_mut() {
                            s.id = Some(id);
                            s.path = Some(path);
                        }
                        crate::window::reload();
                    }
                    Err(err) => me.toast(&err.to_string()),
                },
            );
        }
        self.0.canvas.queue_draw();
        if prefs::get().auto_redact {
            self.redact();
        }
    }

    fn load_saved(&self, original: RgbaImage, state: EditState, id: String, path: PathBuf) {
        let preview_src = annot::preview(&original, WORK_EDGE);
        let work = annot::render(&preview_src, &state);
        self.0.session.replace(Some(Session {
            original,
            preview_src,
            work,
            history: History::from_state(state),
            tool: Tool::Arrow,
            colour: theme_swatches().into_iter().nth(2).unwrap_or(Colour::rgb(0.4, 0.6, 1.0)),
            size: 6.0,
            draft: None,
            image_rect: (0.0, 0.0, 1.0, 1.0),
            path: Some(path),
            id: Some(id),
        }));
        self.0.canvas.queue_draw();
    }

    fn set_tool(&self, tool: Tool) {
        if let Some(s) = self.0.session.borrow_mut().as_mut() {
            s.tool = tool;
            s.draft = None;
        }
        let cursor = if tool == Tool::Text { "text" } else { "crosshair" };
        self.0.canvas.set_cursor_from_name(Some(cursor));
        self.0.canvas.queue_draw();
    }

    fn set_colour(&self, colour: Colour) {
        if let Some(s) = self.0.session.borrow_mut().as_mut() {
            s.colour = colour;
        }
    }

    fn commit(&self, change: impl FnOnce(&mut EditState)) {
        let mut slot = self.0.session.borrow_mut();
        let Some(s) = slot.as_mut() else { return };
        let mut st = s.history.current().clone();
        change(&mut st);
        s.history.push(st);
        s.work = annot::render(&s.preview_src, s.history.current());
        drop(slot);
        self.0.canvas.queue_draw();
        self.write_sidecar();
    }

    fn undo(&self) {
        if let Some(s) = self.0.session.borrow_mut().as_mut() {
            if !s.history.can_undo() {
                return;
            }
            s.history.undo();
            s.work = annot::render(&s.preview_src, s.history.current());
        }
        self.0.canvas.queue_draw();
    }

    fn redo(&self) {
        if let Some(s) = self.0.session.borrow_mut().as_mut() {
            if !s.history.can_redo() {
                return;
            }
            s.history.redo();
            s.work = annot::render(&s.preview_src, s.history.current());
        }
        self.0.canvas.queue_draw();
    }

    fn copy_flat(&self) {
        let Some(sess) = self.0.session.borrow().as_ref().map(|s| (s.original.clone(), s.history.current().clone())) else { return };
        let me = self.clone();
        crate::cmd::background(
            move || {
                let img = annot::render(&sess.0, &sess.1);
                let dynimg = image::DynamicImage::ImageRgba8(img);
                let bytes = export::png_bytes(&dynimg)?;
                clipboard::copy_png(&bytes)?;
                anyhow::Ok(())
            },
            move |result| match result {
                Ok(()) => me.toast("Copied"),
                Err(err) => me.toast(&err.to_string()),
            },
        );
        self.write_sidecar();
    }

    fn save_flat(&self) {
        let Some((original, state)) = self.0.session.borrow().as_ref().map(|s| (s.original.clone(), s.history.current().clone())) else { return };
        let path = nixe_capture_core::action::default_save_path();
        let me = self.clone();
        crate::cmd::background(
            move || {
                let img = annot::render(&original, &state);
                export::write_png(&image::DynamicImage::ImageRgba8(img), &path)?;
                anyhow::Ok(path)
            },
            move |result| match result {
                Ok(path) => me.toast(&format!("Saved {}", path.display())),
                Err(err) => me.toast(&err.to_string()),
            },
        );
        self.write_sidecar();
    }

    fn pin(&self) {
        let img = {
            let slot = self.0.session.borrow();
            let Some(s) = slot.as_ref() else { return };
            annot::render(&s.original, s.history.current())
        };
        crate::pin::open_rgba(img);
        self.write_sidecar();
    }

    fn redact(&self) {
        let Some((original, id)) = self.0.session.borrow().as_ref().map(|s| (s.original.clone(), s.id.clone())) else { return };
        let langs = prefs::get().ocr_langs;
        let mode = OcrMode::parse(&prefs::get().ocr_mode);
        let me = self.clone();
        crate::cmd::background(
            move || {
                let img = image::DynamicImage::ImageRgba8(original);
                let r = recognize(&img, mode, &langs)?;
                let added = nixe_capture_core::redact::marks(&r, img.width(), img.height());
                anyhow::Ok((added, id, r.text))
            },
            move |result| match result {
                Ok((added, _, _)) if added.is_empty() => me.toast("Nothing to redact"),
                Ok((added, id, _)) => {
                    me.commit(|st| st.marks.extend(added));
                    if let Some(id) = id {
                        crate::cmd::background(
                            move || store::open(&paths::library_db())?.set_sensitive(&id, true),
                            |_| crate::window::reload(),
                        );
                    }
                    me.toast("Redacted");
                }
                Err(err) => me.toast(&err.to_string()),
            },
        );
    }

    fn ocr(&self) {
        let Some(original) = self.0.session.borrow().as_ref().map(|s| s.original.clone()) else { return };
        let shown = original.clone();
        let id = self.0.session.borrow().as_ref().and_then(|s| s.id.clone());
        let langs = prefs::get().ocr_langs;
        let mode = OcrMode::parse(&prefs::get().ocr_mode);
        let me = self.clone();
        crate::cmd::background(
            move || {
                let r = recognize(&image::DynamicImage::ImageRgba8(original), mode, &langs)?;
                if let Some(id) = id.filter(|_| !r.text.trim().is_empty()) {
                    let db = store::open(&paths::library_db())?;
                    db.set_ocr(&id, &r.engine, &r.text)?;
                }
                anyhow::Ok(r)
            },
            move |result| match result {
                Ok(r) => {
                    if r.text.trim().is_empty() {
                        me.toast("No text found");
                    } else {
                        crate::window::reload();
                        crate::ocr_view::show(&me.0.window, &shown, &r);
                    }
                }
                Err(err) => me.toast(&err.to_string()),
            },
        );
    }

    fn write_sidecar(&self) {
        let Some((path, state)) = self.0.session.borrow().as_ref().and_then(|s| Some((s.path.clone()?, s.history.current().clone()))) else { return };
        crate::cmd::background(
            move || state.save(&annot::sidecar_path(&path)),
            |_| {},
        );
    }

    fn draw(&self, cr: &cairo::Context, ww: f64, wh: f64) {
        let prepared = {
            let s = self.0.session.borrow();
            let Some(s) = s.as_ref() else { return };
            let (iw, ih) = (s.work.width() as f64, s.work.height() as f64);
            if iw < 1.0 || ih < 1.0 {
                return;
            }
            let scale = ((ww - PAD * 2.0) / iw).min((wh - PAD * 2.0) / ih).max(0.01);
            let dw = iw * scale;
            let dh = ih * scale;
            let ox = (ww - dw) / 2.0;
            let oy = (wh - dh) / 2.0;
            let tool = s.tool;
            let colour = s.colour;
            let size = s.size;
            let draft = s.draft.as_ref().and_then(|d| draft_mark(d, tool, colour, mark_size(tool, size)));
            let crop_draft = match (&s.draft, tool) {
                (Some(Draft::Shape { from, to }), Tool::Crop) => Some((*from, *to)),
                _ => None,
            };
            let surface = rgba_surface(&s.work);
            (iw, ih, scale, dw, dh, ox, oy, surface, draft, crop_draft)
        };
        let (iw, ih, scale, dw, dh, ox, oy, surface, draft, crop_draft) = prepared;
        if let Some(sess) = self.0.session.borrow_mut().as_mut() {
            sess.image_rect = (ox, oy, dw, dh);
        }
        if let Some(surface) = surface {
            let _ = cr.save();
            cr.translate(ox, oy);
            cr.scale(scale, scale);
            if cr.set_source_surface(&surface, 0.0, 0.0).is_ok() {
                cr.source().set_filter(cairo::Filter::Nearest);
                let _ = cr.paint();
            }
            if let Some(m) = draft {
                annot::draw_marks(cr, iw, ih, &[m]);
            }
            let _ = cr.restore();
        }
        if let Some((from, to)) = crop_draft {
            let r = NormRect::from_corners(from, to).scaled(dw, dh);
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.45);
            cr.rectangle(ox, oy, dw, dh);
            cr.rectangle(ox + r.x, oy + r.y, r.w, r.h);
            cr.set_fill_rule(cairo::FillRule::EvenOdd);
            let _ = cr.fill();
            if let Some(accent) = parse_colour(&crate::theme::palette().accent) {
                cr.set_source_rgba(accent.r, accent.g, accent.b, 1.0);
                cr.set_line_width(1.0);
                cr.rectangle(ox + r.x + 0.5, oy + r.y + 0.5, r.w - 1.0, r.h - 1.0);
                let _ = cr.stroke();
            }
        }
    }

    fn wire_pointer(&self) {
        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let me = self.clone();
        drag.connect_drag_begin(move |_, x, y| me.drag_begin(x, y));
        let me = self.clone();
        drag.connect_drag_update(move |g, dx, dy| {
            if let Some((x, y)) = g.start_point() {
                me.drag_update(x + dx, y + dy);
            }
        });
        let me = self.clone();
        drag.connect_drag_end(move |g, dx, dy| {
            if let Some((x, y)) = g.start_point() {
                me.drag_end(x, y, dx, dy);
            }
        });
        self.0.canvas.add_controller(drag);
    }

    fn wire_keys(&self) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = self.clone();
        keys.connect_key_pressed(move |_, key, _, mods| {
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
            let typing = gtk::prelude::GtkWindowExt::focus(&me.0.window).is_some_and(|f| f.dynamic_cast_ref::<gtk::Editable>().is_some());
            if typing {
                return glib::Propagation::Proceed;
            }
            match key {
                gdk::Key::Escape => {
                    me.0.window.close();
                    glib::Propagation::Stop
                }
                gdk::Key::z if ctrl && shift => {
                    me.redo();
                    glib::Propagation::Stop
                }
                gdk::Key::z if ctrl => {
                    me.undo();
                    glib::Propagation::Stop
                }
                gdk::Key::y if ctrl => {
                    me.redo();
                    glib::Propagation::Stop
                }
                gdk::Key::c if ctrl => {
                    me.copy_flat();
                    glib::Propagation::Stop
                }
                gdk::Key::s if ctrl => {
                    me.save_flat();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.0.window.add_controller(keys);
    }

    fn drag_begin(&self, x: f64, y: f64) {
        let mut slot = self.0.session.borrow_mut();
        let Some(s) = slot.as_mut() else { return };
        let Some(p) = norm(s.image_rect, x, y) else { return };
        s.draft = Some(match s.tool {
            Tool::Pen | Tool::Highlighter => Draft::Stroke(vec![p]),
            _ => Draft::Shape { from: p, to: p },
        });
        drop(slot);
        self.0.canvas.queue_draw();
    }

    fn drag_update(&self, x: f64, y: f64) {
        let mut slot = self.0.session.borrow_mut();
        let Some(s) = slot.as_mut() else { return };
        let Some(p) = norm(s.image_rect, x, y) else { return };
        match &mut s.draft {
            Some(Draft::Stroke(points)) => {
                if points.last().is_none_or(|l| (l.0 - p.0).hypot(l.1 - p.1) > 0.002) {
                    points.push(p);
                }
            }
            Some(Draft::Shape { to, .. }) => *to = p,
            None => {}
        }
        drop(slot);
        self.0.canvas.queue_draw();
    }

    fn drag_end(&self, x: f64, y: f64, dx: f64, dy: f64) {
        let (draft, tool, colour, slider) = {
            let mut slot = self.0.session.borrow_mut();
            let Some(s) = slot.as_mut() else { return };
            let draft = s.draft.take();
            (draft, s.tool, s.colour, s.size)
        };
        let size = mark_size(tool, slider);
        match (draft, tool) {
            (Some(Draft::Stroke(points)), Tool::Pen | Tool::Highlighter) => {
                self.commit(|st| st.marks.push(Mark::Stroke { points, colour, size, highlighter: tool == Tool::Highlighter }));
            }
            (Some(Draft::Shape { from, to }), Tool::Arrow) if (from.0 - to.0).hypot(from.1 - to.1) > 0.01 => {
                self.commit(|st| st.marks.push(Mark::Arrow { from, to, colour, size }));
            }
            (Some(Draft::Shape { from, to }), Tool::Line) if (from.0 - to.0).hypot(from.1 - to.1) > 0.01 => {
                self.commit(|st| st.marks.push(Mark::Line { from, to, colour, size }));
            }
            (Some(Draft::Shape { from, to }), Tool::Rect | Tool::Ellipse | Tool::Blur | Tool::Pixelate | Tool::Crop) => {
                let r = NormRect::from_corners(from, to);
                if r.w > 0.005 && r.h > 0.005 {
                    match tool {
                        Tool::Rect => self.commit(|st| st.marks.push(Mark::Rect { rect: r, colour, size })),
                        Tool::Ellipse => self.commit(|st| st.marks.push(Mark::Ellipse { rect: r, colour, size })),
                        Tool::Blur => self.commit(|st| st.marks.push(Mark::Blur { rect: r, size })),
                        Tool::Pixelate => self.commit(|st| st.marks.push(Mark::Pixelate { rect: r, size })),
                        Tool::Crop => self.commit(|st| st.set_crop(Some(r))),
                        _ => self.0.canvas.queue_draw(),
                    }
                } else {
                    self.0.canvas.queue_draw();
                }
            }
            (Some(Draft::Shape { from, .. }), Tool::Text) if dx.hypot(dy) < 6.0 => {
                self.ask_text(x, y, from);
            }
            (Some(Draft::Shape { from, .. }), Tool::Callout) if dx.hypot(dy) < 8.0 => {
                self.commit(|st| {
                    let n = st.next_callout();
                    st.marks.push(Mark::Callout { at: from, n, colour });
                });
            }
            _ => self.0.canvas.queue_draw(),
        }
    }

    fn ask_text(&self, x: f64, y: f64, at: (f64, f64)) {
        let popover = gtk::Popover::new();
        popover.set_parent(&self.0.canvas);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("Type, then press Enter"));
        entry.set_width_chars(24);
        popover.set_child(Some(&entry));
        let (me, pop) = (self.clone(), popover.clone());
        entry.connect_activate(move |e| {
            let text = e.text().trim().to_string();
            pop.popdown();
            if text.is_empty() {
                return;
            }
            let Some((colour, slider)) = me.0.session.borrow().as_ref().map(|s| (s.colour, s.size)) else { return };
            let size = mark_size(Tool::Text, slider);
            me.commit(|st| st.marks.push(Mark::Text { at, text, colour, size }));
        });
        popover.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        popover.popup();
        entry.grab_focus();
    }

    fn toast(&self, message: &str) {
        let overlay = self.0.toast_host.clone();
        let label = gtk::Label::new(Some(message));
        let bx = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        bx.add_css_class("toast");
        bx.set_halign(gtk::Align::Center);
        bx.set_valign(gtk::Align::End);
        bx.set_margin_bottom(56);
        bx.append(&label);
        overlay.add_overlay(&bx);
        glib::timeout_add_local_once(std::time::Duration::from_millis(2200), move || {
            if bx.parent().is_some() {
                overlay.remove_overlay(&bx);
            }
        });
    }
}

fn theme_swatches() -> Vec<Colour> {
    let p = crate::theme::palette();
    let accent = parse_colour(&p.accent).unwrap_or(Colour::rgb(0.4, 0.6, 1.0));
    let danger = parse_colour(&p.danger).unwrap_or(accent);
    let text = parse_colour(&p.text).unwrap_or(Colour::rgb(1.0, 1.0, 1.0));
    let bg = parse_colour(&p.bg).unwrap_or(Colour::rgb(0.1, 0.1, 0.12));
    let mix = |a: Colour, b: Colour, t: f64| Colour::rgb(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t);
    vec![text, bg, accent, danger, mix(accent, text, 0.4), mix(danger, accent, 0.45), mix(accent, bg, 0.3), mix(danger, text, 0.25)]
}

fn parse_colour(hex: &str) -> Option<Colour> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Colour::rgb(((v >> 16) & 0xff) as f64 / 255.0, ((v >> 8) & 0xff) as f64 / 255.0, (v & 0xff) as f64 / 255.0))
}

fn walk_swatches(row: &gtk::Box, on: impl Fn(Colour) + Clone + 'static) {
    let mut child = row.first_child();
    while let Some(next) = child {
        if let Some(btn) = next.downcast_ref::<gtk::Button>() {
            let name = btn.widget_name().to_string();
            if let Some(c) = parse_colour(&name) {
                let on = on.clone();
                btn.connect_clicked(move |_| on(c));
            }
        }
        child = next.next_sibling();
    }
}

fn mark_size(tool: Tool, slider: f64) -> f64 {
    match tool {
        Tool::Pen | Tool::Highlighter | Tool::Arrow | Tool::Line | Tool::Rect | Tool::Ellipse => slider * 0.002,
        Tool::Text => slider * 0.008,
        Tool::Blur | Tool::Pixelate | Tool::Crop | Tool::Callout => slider * 0.006,
    }
}

fn draft_mark(draft: &Draft, tool: Tool, colour: Colour, size: f64) -> Option<Mark> {
    match (draft, tool) {
        (Draft::Stroke(points), Tool::Pen | Tool::Highlighter) => {
            Some(Mark::Stroke { points: points.clone(), colour, size, highlighter: tool == Tool::Highlighter })
        }
        (Draft::Shape { from, to }, Tool::Arrow) => Some(Mark::Arrow { from: *from, to: *to, colour, size }),
        (Draft::Shape { from, to }, Tool::Line) => Some(Mark::Line { from: *from, to: *to, colour, size }),
        (Draft::Shape { from, to }, Tool::Rect) => Some(Mark::Rect { rect: NormRect::from_corners(*from, *to), colour, size }),
        (Draft::Shape { from, to }, Tool::Ellipse) => Some(Mark::Ellipse { rect: NormRect::from_corners(*from, *to), colour, size }),
        _ => None,
    }
}

fn norm(rect: (f64, f64, f64, f64), x: f64, y: f64) -> Option<(f64, f64)> {
    let (ox, oy, w, h) = rect;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some((((x - ox) / w).clamp(0.0, 1.0), ((y - oy) / h).clamp(0.0, 1.0)))
}

fn rgba_surface(img: &RgbaImage) -> Option<cairo::ImageSurface> {
    let (w, h) = img.dimensions();
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().ok()?;
        for (y, row) in img.rows().enumerate() {
            for (x, p) in row.enumerate() {
                let a = p.0[3] as u32;
                let m = |c: u8| (c as u32 * a + 127) / 255;
                let px = (a << 24) | (m(p.0[0]) << 16) | (m(p.0[1]) << 8) | m(p.0[2]);
                data[y * stride + x * 4..][..4].copy_from_slice(&px.to_ne_bytes());
            }
        }
    }
    Some(surface)
}
