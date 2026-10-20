//! Theming: the shared Nixe theme engine, given this app's stylesheet.

pub use nixe_theme::*;

use crate::prefs;
use gtk::gdk;

pub fn install() {
    install_icons();
    nixe_theme::install(Style {
        window_class: "capture-window",
        base_skip: &[],
        css: include_str!("style.css"),
        no_glow_css: "",
        extra_colors: None,
        prefs: || {
            let p = prefs::get();
            ThemePrefs { mode: p.mode, theme: p.theme, glow: p.glow, reduce_motion: p.reduce_motion }
        },
        set_theme: Some(|mode, theme| {
            prefs::update(|p| {
                p.mode = mode;
                p.theme = theme;
            });
        }),
    });
}

fn install_icons() {
    const ICONS: &[(&str, &str)] = &[
        ("capture-region-symbolic.svg", include_str!("../data/icons/capture-region-symbolic.svg")),
        ("capture-tool-arrow-symbolic.svg", include_str!("../data/icons/capture-tool-arrow-symbolic.svg")),
        ("capture-tool-line-symbolic.svg", include_str!("../data/icons/capture-tool-line-symbolic.svg")),
        ("capture-tool-rect-symbolic.svg", include_str!("../data/icons/capture-tool-rect-symbolic.svg")),
        ("capture-tool-ellipse-symbolic.svg", include_str!("../data/icons/capture-tool-ellipse-symbolic.svg")),
        ("capture-tool-highlighter-symbolic.svg", include_str!("../data/icons/capture-tool-highlighter-symbolic.svg")),
        ("capture-tool-callout-symbolic.svg", include_str!("../data/icons/capture-tool-callout-symbolic.svg")),
    ];
    let dir = crate::paths::cache_dir().join("icons");
    for (name, svg) in ICONS {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = crate::cmd::atomic_write(&path, svg.as_bytes());
        }
    }
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}
