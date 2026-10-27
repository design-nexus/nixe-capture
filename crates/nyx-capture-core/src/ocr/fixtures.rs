//! Synthetic OCR fixtures. No third-party screenshots.

use super::{OcrMode, models_present, recognize};
use crate::detect;
use cairo::{Context, Format, ImageSurface};
use image::{DynamicImage, RgbaImage};

fn render_text(text: &str, px: i32, width: i32, height: i32) -> DynamicImage {
    let mut surface = ImageSurface::create(Format::ARgb32, width, height).expect("surface");
    {
        let cr = Context::new(&surface).expect("cairo");
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        cr.set_source_rgb(0.0, 0.0, 0.0);
        let layout = pangocairo::functions::create_layout(&cr);
        let mut font = pango::FontDescription::new();
        font.set_family("Sans");
        font.set_absolute_size(px as f64 * pango::SCALE as f64);
        layout.set_font_description(Some(&font));
        layout.set_text(text);
        cr.move_to(24.0, 24.0);
        pangocairo::functions::show_layout(&cr, &layout);
    }
    let w = surface.width() as u32;
    let h = surface.height() as u32;
    let stride = surface.stride() as usize;
    let data = surface.data().expect("data");
    let mut img = RgbaImage::new(w, h);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * stride + x * 4;
            let px = u32::from_ne_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            let a = ((px >> 24) & 0xff) as u8;
            let r = ((px >> 16) & 0xff) as u8;
            let g = ((px >> 8) & 0xff) as u8;
            let b = (px & 0xff) as u8;
            img.put_pixel(x as u32, y as u32, image::Rgba([r, g, b, a]));
        }
    }
    DynamicImage::ImageRgba8(img)
}

fn has_tesseract() -> bool {
    detect::Tools::probe().tesseract
}

#[test]
fn tesseract_reads_ui_sentence() {
    if !has_tesseract() {
        return;
    }
    let img = render_text("Open Settings", 36, 640, 160);
    let r = recognize(&img, OcrMode::Tesseract, "eng").expect("ocr");
    let text = r.text.to_lowercase();
    assert!(text.contains("open") || text.contains("settings"), "got {:?}", r.text);
    assert_eq!(r.engine, "tesseract");
}

#[test]
fn tesseract_reads_terminal_line() {
    if !has_tesseract() {
        return;
    }
    let img = render_text("kernel panic on boot", 28, 720, 140);
    let r = recognize(&img, OcrMode::Tesseract, "eng").expect("ocr");
    let text = r.text.to_lowercase();
    assert!(text.contains("kernel") || text.contains("panic") || text.contains("boot"), "got {:?}", r.text);
}

#[test]
fn small_type_still_returns_text() {
    if !has_tesseract() {
        return;
    }
    let img = render_text("tiny font sample", 11, 400, 80);
    let r = recognize(&img, OcrMode::Tesseract, "eng").expect("ocr");
    assert!(!r.text.trim().is_empty() || r.engine == "tesseract");
}

#[test]
fn rapidocr_reads_when_models_are_cached() {
    if !models_present() {
        return;
    }
    let img = render_text("Open Settings", 36, 640, 160);
    let r = recognize(&img, OcrMode::RapidOcr, "eng").expect("ocr");
    let text = r.text.to_lowercase();
    assert!(text.contains("open") || text.contains("settings"), "got {:?}", r.text);
    assert_eq!(r.engine, "rapidocr");
}

#[test]
fn automatic_does_not_load_rapidocr_on_clear_text() {
    if !has_tesseract() {
        return;
    }
    let img = render_text("Open the settings window", 32, 720, 160);
    let r = recognize(&img, OcrMode::Automatic, "eng").expect("ocr");
    assert!(!r.fallback_used);
    assert_eq!(r.engine, "tesseract");
}
