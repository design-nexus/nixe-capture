//! Virtualized history grid. Only on-screen cells exist.

use crate::paths;
use crate::widgets;
use crate::window;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use nixe_capture_core::store::{self, CaptureRow, THUMB_EDGE};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};


const CACHE_LIMIT: usize = 256;

#[derive(Default)]
struct TextureCache {
    map: HashMap<String, gdk::Texture>,
    order: VecDeque<String>,
}

impl TextureCache {
    fn get(&mut self, key: &str) -> Option<gdk::Texture> {
        let tex = self.map.get(key)?.clone();
        if let Some(at) = self.order.iter().rposition(|k| k == key) {
            self.order.remove(at);
        }
        self.order.push_back(key.to_string());
        Some(tex)
    }

    fn insert(&mut self, key: String, tex: gdk::Texture) {
        if self.map.insert(key.clone(), tex).is_none() {
            self.order.push_back(key);
        }
        while self.map.len() > CACHE_LIMIT {
            let Some(old) = self.order.pop_front() else { break };
            self.map.remove(&old);
        }
    }
}

thread_local! {
    static TEXTURES: RefCell<TextureCache> = RefCell::default();
}

pub fn build() -> (gtk::ScrolledWindow, gio::ListStore) {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, obj| {
        let item = obj.downcast_ref::<gtk::ListItem>().expect("grid item");
        let picture = gtk::Picture::new();
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_size_request(168, 126);

        let star = widgets::icon_button("non-starred-symbolic", "Star");
        star.add_css_class("history-star");
        star.set_halign(gtk::Align::End);
        star.set_valign(gtk::Align::Start);
        star.set_margin_top(4);
        star.set_margin_end(4);
        let steal = gtk::GestureClick::new();
        steal.set_button(1);
        steal.set_propagation_phase(gtk::PropagationPhase::Capture);
        steal.connect_released(|g, _, _, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            let Some(b) = g.widget().and_downcast::<gtk::Button>() else { return };
            let Some(root) = b.ancestor(gtk::Overlay::static_type()) else { return };
            let id = root.widget_name().to_string();
            if id.is_empty() {
                return;
            }
            let on = b.icon_name().as_deref() != Some("starred-symbolic");
            window::set_starred(&id, on);
        });
        star.add_controller(steal);

        let overlay = gtk::Overlay::new();
        overlay.add_css_class("history-cell");
        overlay.set_child(Some(&picture));
        overlay.add_overlay(&star);

        let right = gtk::GestureClick::new();
        right.set_button(3);
        right.connect_pressed(|g, _, x, y| {
            let Some(w) = g.widget() else { return };
            let id = w.widget_name().to_string();
            if id.is_empty() {
                return;
            }
            window::item_menu(&id, &w, x, y);
        });
        overlay.add_controller(right);

        item.set_child(Some(&overlay));
    });
    factory.connect_bind(|_, obj| {
        let item = obj.downcast_ref::<gtk::ListItem>().expect("grid item");
        let Some(value) = item.item().and_downcast::<glib::BoxedAnyObject>() else { return };
        let row = value.borrow::<CaptureRow>().clone();
        let Some(overlay) = item.child().and_downcast::<gtk::Overlay>() else { return };
        overlay.set_widget_name(&row.id);
        overlay.set_tooltip_text(Some(&tooltip(&row)));
        let Some(picture) = overlay.child().and_downcast::<gtk::Picture>() else { return };
        if let Some(star) = overlay_star(&overlay) {
            let on = row.starred;
            star.set_icon_name(if on { "starred-symbolic" } else { "non-starred-symbolic" });
            star.set_tooltip_text(Some(if on { "Unstar" } else { "Star" }));
        }
        fill_picture(&picture, &row.id, &row.path);
    });
    factory.connect_unbind(|_, obj| {
        let item = obj.downcast_ref::<gtk::ListItem>().expect("grid item");
        if let Some(overlay) = item.child().and_downcast::<gtk::Overlay>()
            && let Some(picture) = overlay.child().and_downcast::<gtk::Picture>()
        {
            picture.set_paintable(None::<&gdk::Paintable>);
        }
    });
    let view = gtk::GridView::new(Some(gtk::NoSelection::new(Some(store.clone()))), Some(factory));
    view.add_css_class("history-grid");
    view.set_min_columns(2);
    view.set_max_columns(8);
    view.set_single_click_activate(true);
    view.set_enable_rubberband(false);
    let data = store.clone();
    view.connect_activate(move |_, position| {
        if let Some(obj) = data.item(position).and_downcast::<glib::BoxedAnyObject>() {
            window::open_capture(obj.borrow::<CaptureRow>().clone());
        }
    });
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vexpand(true)
        .hexpand(true)
        .child(&view)
        .build();
    (scroll, store)
}

pub fn fill(store: &gio::ListStore, items: &[CaptureRow]) {
    store.remove_all();
    let objs: Vec<glib::BoxedAnyObject> = items.iter().cloned().map(glib::BoxedAnyObject::new).collect();
    store.extend_from_slice(&objs);
}

fn overlay_star(overlay: &gtk::Overlay) -> Option<gtk::Button> {
    let mut child = overlay.first_child();
    while let Some(next) = child {
        if let Ok(btn) = next.clone().downcast::<gtk::Button>() {
            return Some(btn);
        }
        child = next.next_sibling();
    }
    None
}

fn tooltip(row: &CaptureRow) -> String {
    if row.width > 0 && row.height > 0 {
        format!("{} × {}", row.width, row.height)
    } else {
        "Capture".into()
    }
}

fn fill_picture(picture: &gtk::Picture, id: &str, path: &std::path::Path) {
    if let Some(tex) = TEXTURES.with(|c| c.borrow_mut().get(id)) {
        picture.set_paintable(Some(&tex));
        return;
    }
    picture.set_paintable(None::<&gdk::Paintable>);
    let picture = picture.clone();
    let id = id.to_string();
    let src = path.to_path_buf();
    let dest = paths::thumb_file(&id);
    crate::cmd::background(
        move || {
            store::write_thumb(&src, &dest, THUMB_EDGE)?;
            anyhow::Ok((id, dest))
        },
        move |result| {
            let Ok((id, dest)) = result else { return };
            let Ok(tex) = gdk::Texture::from_filename(&dest) else { return };
            TEXTURES.with(|c| c.borrow_mut().insert(id, tex.clone()));
            picture.set_paintable(Some(&tex));
        },
    );
}

pub fn forget_thumb(id: &str) {
    TEXTURES.with(|c| {
        let mut cache = c.borrow_mut();
        cache.map.remove(id);
        cache.order.retain(|k| k != id);
    });
    let _ = std::fs::remove_file(paths::thumb_file(id));
}
