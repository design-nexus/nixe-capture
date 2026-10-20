//! The settings card's groups.

use crate::prefs;
use crate::theme;
use crate::widgets::{self, Page};
use crate::window;
use gtk::gio;
use gtk::prelude::*;
use nixe_capture_core::action;
use nixe_capture_core::detect::Tools;
use std::path::Path;

pub fn build(page: &Page) {
    window_group(page);
    capture_group(page);
    history_group(page);
    privacy_group(page);
    ocr_group(page);
    record_group(page);
    keyboard(page);
}

fn window_group(page: &Page) {
    let group = page.group("Capture window");
    group.add(&nixe_ui::widgets::start_row());
    let prefs = prefs::get();
    let options: Vec<(String, String)> = theme::all().iter().map(|t| (t.id.clone(), t.name.clone())).collect();
    let (theme_row, theme_dd) =
        widgets::choice_row("Theme", "A bundled theme, or one of your own.", options, &prefs.theme, |id| {
            prefs::update(|p| {
                p.theme = id;
                p.mode = prefs::ThemeMode::Theme;
            });
            theme::apply();
        });
    theme_dd.set_sensitive(prefs.mode == prefs::ThemeMode::Theme || !theme::omarchy_available());
    if theme::omarchy_available() {
        let dd = theme_dd.clone();
        let (row, _) = widgets::switch_row(
            "Follow Omarchy theme",
            "Match the desktop's colors and update when the Omarchy theme changes.",
            prefs.mode == prefs::ThemeMode::Omarchy,
            move |on| {
                prefs::update(|p| p.mode = if on { prefs::ThemeMode::Omarchy } else { prefs::ThemeMode::Theme });
                dd.set_sensitive(!on);
                theme::apply();
            },
        );
        group.add(&row);
    }
    group.add(&theme_row);
    let (row, _) = widgets::switch_row("Glow", "Soft accent glow around focused and selected elements.", prefs.glow, |on| {
        prefs::update(|p| p.glow = on);
        theme::apply();
    });
    group.add(&row);
    let (row, _) = widgets::switch_row("Reduce motion", "Turn off transitions in this window.", prefs.reduce_motion, |on| {
        prefs::update(|p| p.reduce_motion = on);
        theme::apply();
    });
    group.add(&row);
}

fn capture_group(page: &Page) {
    let group = page.group("Capture");
    let prefs = prefs::get();
    let tools = Tools::probe();
    let mut after = vec![("edit", "Edit"), ("save", "Save"), ("pin", "Pin")];
    if tools.wl_copy {
        after.insert(1, ("copy", "Copy"));
    }
    if tools.can_ocr() {
        after.push(("ocr", "OCR"));
    }
    group.add(&widgets::segmented_row(
        "After a screenshot",
        "What to do once the region is chosen.",
        widgets::opts(&after),
        &prefs.after_capture,
        |v| prefs::update(|p| p.after_capture = v),
    ));
    let save_label = save_dir_label(&prefs.save_dir);
    let (row, _) = widgets::button_row(
        "Save folder",
        "Where Save writes PNG files. Empty uses Pictures / Nixe Capture.",
        &save_label,
        |btn| choose_save_dir(btn.clone()),
    );
    group.add(&row);
    let mut selector = vec![("auto", "Auto")];
    if crate::overlay::supported() {
        selector.push(("overlay", "Overlay"));
    }
    if tools.slurp {
        selector.push(("slurp", "slurp"));
    }
    if selector.len() > 1 {
        group.add(&widgets::segmented_row(
            "Selector",
            "The GTK overlay on Hyprland, or slurp when layer-shell is missing.",
            widgets::opts(&selector),
            &prefs.selector,
            |v| prefs::update(|p| p.selector = v),
        ));
    }
    let mut backends = vec![("auto", "Auto")];
    if tools.grim {
        backends.push(("grim", "grim"));
    }
    if tools.portal {
        backends.push(("portal", "Portal"));
    }
    if backends.len() > 1 {
        group.add(&widgets::segmented_row(
            "Backend",
            "grim on Hyprland. The XDG screenshot portal when grim is missing.",
            widgets::opts(&backends),
            &prefs.backend,
            |v| prefs::update(|p| p.backend = v),
        ));
    }
}

fn save_dir_label(dir: &str) -> String {
    if dir.is_empty() {
        return "Pictures / Nixe Capture".into();
    }
    Path::new(dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| dir.to_string())
}

fn choose_save_dir(btn: gtk::Button) {
    let dialog = gtk::FileDialog::builder().title("Save screenshots here").modal(true).build();
    let start = {
        let dir = prefs::get().save_dir;
        if dir.is_empty() { action::default_save_dir() } else { Path::new(&dir).to_path_buf() }
    };
    if start.is_dir() {
        dialog.set_initial_folder(Some(&gio::File::for_path(start)));
    }
    dialog.select_folder(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
        let Ok(file) = res else { return };
        let Some(path) = file.path() else { return };
        let text = path.to_string_lossy().into_owned();
        btn.set_label(&save_dir_label(&text));
        prefs::update(|p| p.save_dir = text);
    });
}

fn history_group(page: &Page) {
    let group = page.group("History");
    let prefs = prefs::get();
    let (row, _) = widgets::switch_row("Keep a history", "Store captures on this computer so they can be searched.", prefs.history, |on| {
        prefs::update(|p| p.history = on);
        window::reload();
    });
    group.add(&row);
    group.add(&widgets::segmented_row(
        "Keep for",
        "Unstarred captures older than this are removed on launch. Starred ones stay.",
        widgets::opts(&[("7", "7 days"), ("30", "30 days"), ("90", "90 days"), ("365", "1 year"), ("0", "Forever")]),
        &prefs.retention_days.to_string(),
        |v| {
            if let Ok(n) = v.parse() {
                prefs::update(|p| p.retention_days = n);
            }
        },
    ));
}

fn privacy_group(page: &Page) {
    let group = page.group("Assistant");
    let prefs = prefs::get();
    let (row, _) = widgets::switch_row(
        "Assistant can send a screenshot",
        "Let Nixe Assistant capture the screen or read OCR from history while Capture is open. Off by default.",
        prefs.assistant_can_send,
        |on| prefs::update(|p| p.assistant_can_send = on),
    );
    group.add(&row);
    let (row, _) = widgets::switch_row(
        "Redact secrets after OCR",
        "Pixelate emails, URLs, phone numbers, cards, and keys found in the text. Stays on this computer.",
        prefs.auto_redact,
        |on| prefs::update(|p| p.auto_redact = on),
    );
    group.add(&row);
    let (row, _) = widgets::button_row("Tutorial", "Show the three-step capture walkthrough again.", "Show", |_| {
        prefs::update(|p| p.tutorial_done = false);
        window::show_tutorial();
    });
    group.add(&row);
}

fn ocr_group(page: &Page) {
    let group = page.group("OCR");
    let prefs = prefs::get();
    group.add(&widgets::segmented_row(
        "Engine",
        "Automatic uses Tesseract first, then RapidOCR when the text looks unreliable.",
        widgets::opts(&[("automatic", "Automatic"), ("tesseract", "Tesseract"), ("rapidocr", "RapidOCR")]),
        &prefs.ocr_mode,
        |v| prefs::update(|p| p.ocr_mode = v),
    ));
    let (row, _) = widgets::switch_row("Read text after capture", "Run OCR in the background and index it for search.", prefs.ocr_on_capture, |on| {
        prefs::update(|p| p.ocr_on_capture = on);
    });
    group.add(&row);
}

fn record_group(page: &Page) {
    let tools = Tools::probe();
    if !tools.can_record() {
        return;
    }
    let group = page.group("Recording");
    let prefs = prefs::get();
    let mut rec = vec![("auto", "Auto")];
    if tools.gpu_screen_recorder {
        rec.push(("gpu-screen-recorder", "GPU Screen Recorder"));
    }
    if tools.wf_recorder {
        rec.push(("wf-recorder", "wf-recorder"));
    }
    if tools.wl_screenrec {
        rec.push(("wl-screenrec", "wl-screenrec"));
    }
    if tools.can_gst_record() {
        rec.push(("pipewire", "PipeWire portal"));
    }
    if rec.len() > 1 {
        group.add(&widgets::segmented_row(
            "Recorder",
            "gpu-screen-recorder first, then wf-recorder, then wl-screenrec, then PipeWire via the ScreenCast portal.",
            widgets::opts(&rec),
            &prefs.recorder,
            |v| prefs::update(|p| p.recorder = v),
        ));
    }
    group.add(&widgets::segmented_row(
        "Audio",
        "Tracks mixed into the file. Off records video only.",
        widgets::opts(&[("none", "Off"), ("desktop", "System"), ("mic", "Mic"), ("both", "Both")]),
        &prefs.record_audio,
        |v| prefs::update(|p| p.record_audio = v),
    ));
    group.add(&widgets::segmented_row(
        "Format",
        "GIF is converted with ffmpeg after the recording stops.",
        widgets::opts(&[("mp4", "MP4"), ("webm", "WebM"), ("gif", "GIF")]),
        &prefs.record_container,
        |v| prefs::update(|p| p.record_container = v),
    ));
    group.add(&widgets::segmented_row(
        "Countdown",
        "Seconds on screen before the recorder starts.",
        widgets::opts(&[("0", "Off"), ("3", "3 s"), ("5", "5 s")]),
        &prefs.record_delay.to_string(),
        |v| {
            if let Ok(n) = v.parse() {
                prefs::update(|p| p.record_delay = n);
            }
        },
    ));
    let (row, _) = widgets::switch_row("Cursor", "Include the pointer in the recording.", prefs.record_cursor, |on| {
        prefs::update(|p| p.record_cursor = on);
    });
    group.add(&row);
    if tools.gpu_screen_recorder {
        let (row, _) = widgets::switch_row(
            "Use screenshot portal",
            "gpu-screen-recorder -w portal. Off uses the focused monitor or a dragged region.",
            prefs.record_use_portal,
            |on| prefs::update(|p| p.record_use_portal = on),
        );
        group.add(&row);
    }
    if tools.can_webcam() {
        let (row, _) = widgets::switch_row(
            "Webcam overlay",
            "mpv picture-in-picture using Omarchy WebcamOverlay window rules.",
            prefs.record_webcam,
            |on| prefs::update(|p| p.record_webcam = on),
        );
        group.add(&row);
        group.add(&widgets::segmented_row(
            "Webcam size",
            "Hyprland places the overlay in the corner. SUPER+ALT [ ] resizes it.",
            widgets::opts(&[("small", "Small"), ("medium", "Medium"), ("large", "Large")]),
            &prefs.record_webcam_size,
            |v| prefs::update(|p| p.record_webcam_size = v),
        ));
        let devices = nixe_capture_core::webcam::list_devices();
        if !devices.is_empty() {
            let mut opts = vec![("".into(), "First camera".into())];
            for d in devices {
                let label = if d.name.is_empty() { d.path.clone() } else { format!("{} ({})", d.name, d.path) };
                opts.push((d.path, label));
            }
            let (row, _) = widgets::choice_row(
                "Webcam",
                "Device for the overlay. First camera is the default.",
                opts,
                &prefs.record_webcam_device,
                |v| prefs::update(|p| p.record_webcam_device = v),
            );
            group.add(&row);
        }
    }
}

fn keyboard(page: &Page) {
    let group = page.group("Keyboard");
    let list = widgets::vbox(0);
    list.add_css_class("group-list");
    for (keys, what) in window::SHORTCUTS {
        list.append(&widgets::row(what, "", Some(widgets::key_caps(keys).upcast_ref())));
    }
    group.add(&list);
}
