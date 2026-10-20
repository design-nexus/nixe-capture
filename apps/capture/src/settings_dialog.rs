//! Settings as a card over the window.

use crate::settings_page;
use crate::widgets;
use crate::window;
use gtk::prelude::*;
use nixe_ui::settings_card::{self, Spec};

pub use nixe_ui::settings_card::fit;

const GROUPS: &[(&str, &str, &str)] = &[
    ("Capture window", "applications-graphics-symbolic", "Theme, glow, motion"),
    ("Capture", "camera-photo-symbolic", "What happens after a shot"),
    ("History", "document-open-recent-symbolic", "Keep, search, and forget captures"),
    ("OCR", "insert-text-symbolic", "Tesseract and RapidOCR"),
    ("Keyboard", "input-keyboard-symbolic", "Shortcuts"),
];

const SPEC: Spec = Spec { groups: GROUPS, search_text: None };

pub fn open() {
    let (Some(overlay), Some(window)) = (window::overlay(), window::window()) else { return };
    settings_card::open(&overlay, &window, SPEC, || {
        let page = widgets::page("settings");
        settings_page::build(&page);
        (page.root.upcast(), page.body)
    });
}
