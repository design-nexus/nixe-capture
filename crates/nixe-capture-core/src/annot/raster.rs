//! Flatten an `EditState` onto pixels. Crop, then blur/pixelate, then cairo strokes.

use super::{Colour, EditState, Mark, NormRect};
use cairo::{self, Context, Format, ImageSurface};
use image::imageops::{self, FilterType};
use image::RgbaImage;
use std::f64::consts::PI;

const HIGHLIGHTER_WIDTH: f64 = 4.0;
const HIGHLIGHTER_ALPHA: f64 = 0.4;

/// Apply `st` to a copy of `src`. `src` is never written.
pub fn render(src: &RgbaImage, st: &EditState) -> RgbaImage {
    let mut img = if let Some(c) = st.crop {
        crop(src, c)
    } else {
        src.clone()
    };
    for m in &st.marks {
        match m {
            Mark::Pixelate { rect, size } => pixelate(&mut img, *rect, *size),
            Mark::Blur { rect, size } => blur(&mut img, *rect, *size),
            _ => {}
        }
    }
    draw_on_image(&mut img, &st.marks);
    img
}

fn crop(img: &RgbaImage, c: NormRect) -> RgbaImage {
    let (w, h) = img.dimensions();
    let x = ((c.x * w as f64).round() as u32).min(w.saturating_sub(1));
    let y = ((c.y * h as f64).round() as u32).min(h.saturating_sub(1));
    let cw = ((c.w * w as f64).round() as u32).clamp(1, w - x);
    let ch = ((c.h * h as f64).round() as u32).clamp(1, h - y);
    imageops::crop_imm(img, x, y, cw, ch).to_image()
}

fn pixelate(img: &mut RgbaImage, rect: NormRect, size: f64) {
    let (w, h) = img.dimensions();
    let (x0, y0, x1, y1) = pixel_box(rect, w, h);
    let block = ((size * w.min(h) as f64).round() as u32).max(2);
    let mut by = y0;
    while by < y1 {
        let bh = block.min(y1 - by);
        let mut bx = x0;
        while bx < x1 {
            let bw = block.min(x1 - bx);
            let mut sum = [0u64; 4];
            for y in by..by + bh {
                for x in bx..bx + bw {
                    for (s, c) in sum.iter_mut().zip(img.get_pixel(x, y).0) {
                        *s += c as u64;
                    }
                }
            }
            let n = (bw * bh) as u64;
            let avg = [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, (sum[3] / n) as u8];
            for y in by..by + bh {
                for x in bx..bx + bw {
                    img.get_pixel_mut(x, y).0 = avg;
                }
            }
            bx += bw;
        }
        by += bh;
    }
}

fn blur(img: &mut RgbaImage, rect: NormRect, size: f64) {
    let (w, h) = img.dimensions();
    let (x0, y0, x1, y1) = pixel_box(rect, w, h);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let bw = x1 - x0;
    let bh = y1 - y0;
    let sub = imageops::crop_imm(img, x0, y0, bw, bh).to_image();
    let sigma = (size as f32 * w.min(h) as f32).clamp(1.0, 48.0);
    let blurred = imageops::blur(&sub, sigma);
    for y in 0..bh {
        for x in 0..bw {
            img.put_pixel(x0 + x, y0 + y, *blurred.get_pixel(x, y));
        }
    }
}

fn pixel_box(rect: NormRect, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let x0 = (rect.x * w as f64).round().clamp(0.0, w as f64) as u32;
    let y0 = (rect.y * h as f64).round().clamp(0.0, h as f64) as u32;
    let x1 = (rect.right() * w as f64).round().clamp(0.0, w as f64) as u32;
    let y1 = (rect.bottom() * h as f64).round().clamp(0.0, h as f64) as u32;
    (x0, y0, x1.max(x0), y1.max(y0))
}

pub fn draw_marks(cr: &Context, w: f64, h: f64, marks: &[Mark]) {
    let unit = w.min(h);
    for mark in marks {
        let _ = cr.save();
        draw_one(cr, w, h, unit, mark);
        let _ = cr.restore();
    }
}

fn set_colour(cr: &Context, c: Colour, alpha: f64) {
    cr.set_source_rgba(c.r, c.g, c.b, alpha);
}

fn draw_one(cr: &Context, w: f64, h: f64, unit: f64, mark: &Mark) {
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    match mark {
        Mark::Stroke { points, colour, size, highlighter } => {
            let Some(first) = points.first() else { return };
            if *highlighter {
                set_colour(cr, *colour, HIGHLIGHTER_ALPHA);
                cr.set_line_width(size * unit * HIGHLIGHTER_WIDTH);
                cr.set_line_cap(cairo::LineCap::Butt);
            } else {
                set_colour(cr, *colour, 1.0);
                cr.set_line_width(size * unit);
            }
            cr.move_to(first.0 * w, first.1 * h);
            cr.line_to(first.0 * w, first.1 * h);
            for p in &points[1..] {
                cr.line_to(p.0 * w, p.1 * h);
            }
            let _ = cr.stroke();
        }
        Mark::Line { from, to, colour, size } => {
            set_colour(cr, *colour, 1.0);
            cr.set_line_width(size * unit);
            cr.move_to(from.0 * w, from.1 * h);
            cr.line_to(to.0 * w, to.1 * h);
            let _ = cr.stroke();
        }
        Mark::Arrow { from, to, colour, size } => {
            let width = size * unit;
            let (fx, fy, tx, ty) = (from.0 * w, from.1 * h, to.0 * w, to.1 * h);
            let angle = (ty - fy).atan2(tx - fx);
            let head = width * 4.0 + unit * 0.01;
            let (bx, by) = (tx - head * 0.8 * angle.cos(), ty - head * 0.8 * angle.sin());
            set_colour(cr, *colour, 1.0);
            cr.set_line_width(width);
            cr.move_to(fx, fy);
            cr.line_to(bx, by);
            let _ = cr.stroke();
            let wing = 25f64.to_radians();
            cr.move_to(tx, ty);
            cr.line_to(tx - head * (angle - wing).cos(), ty - head * (angle - wing).sin());
            cr.line_to(tx - head * (angle + wing).cos(), ty - head * (angle + wing).sin());
            cr.close_path();
            let _ = cr.fill();
        }
        Mark::Rect { rect, colour, size } => {
            let r = rect.scaled(w, h);
            set_colour(cr, *colour, 1.0);
            cr.set_line_width(size * unit);
            cr.rectangle(r.x, r.y, r.w, r.h);
            let _ = cr.stroke();
        }
        Mark::Ellipse { rect, colour, size } => {
            let r = rect.scaled(w, h);
            if r.w < 0.5 || r.h < 0.5 {
                return;
            }
            set_colour(cr, *colour, 1.0);
            cr.translate(r.x + r.w / 2.0, r.y + r.h / 2.0);
            cr.scale(r.w / 2.0, r.h / 2.0);
            cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
            cr.identity_matrix();
            cr.set_line_width(size * unit);
            let _ = cr.stroke();
        }
        Mark::Text { at, text, colour, size } => {
            if text.is_empty() {
                return;
            }
            paint_label(cr, at.0 * w, at.1 * h, text, *colour, (size * unit).max(4.0));
        }
        Mark::Callout { at, n, colour } => {
            let cx = at.0 * w;
            let cy = at.1 * h;
            let r = (unit * 0.028).max(8.0);
            set_colour(cr, *colour, 1.0);
            cr.arc(cx, cy, r, 0.0, 2.0 * PI);
            let _ = cr.fill_preserve();
            set_colour(cr, colour.contrast(), 0.85);
            cr.set_line_width(1.5);
            let _ = cr.stroke();
            paint_label(cr, cx - r * 0.45, cy - r * 0.55, &n.to_string(), colour.contrast(), r * 0.95);
        }
        Mark::Pixelate { .. } | Mark::Blur { .. } => {}
    }
}

fn paint_label(cr: &Context, x: f64, y: f64, text: &str, colour: Colour, px: f64) {
    let layout = pangocairo::functions::create_layout(cr);
    let mut font = pango::FontDescription::new();
    font.set_family("Sans");
    font.set_weight(pango::Weight::Bold);
    font.set_absolute_size(px * pango::SCALE as f64);
    layout.set_font_description(Some(&font));
    layout.set_text(text);
    cr.move_to(x, y);
    pangocairo::functions::layout_path(cr, &layout);
    set_colour(cr, colour.contrast(), 0.6);
    cr.set_line_width(px * 0.14);
    let _ = cr.stroke_preserve();
    set_colour(cr, colour, 1.0);
    let _ = cr.fill();
}

fn surface_from_image(img: &RgbaImage) -> Option<ImageSurface> {
    let (w, h) = img.dimensions();
    let mut surface = ImageSurface::create(Format::ARgb32, w as i32, h as i32).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().ok()?;
        for (y, row) in img.rows().enumerate() {
            for (x, p) in row.enumerate() {
                data[y * stride + x * 4..][..4].copy_from_slice(&premultiply(p.0).to_ne_bytes());
            }
        }
    }
    Some(surface)
}

fn copy_surface_into(surface: &mut ImageSurface, img: &mut RgbaImage) {
    surface.flush();
    let stride = surface.stride() as usize;
    if let Ok(data) = surface.data() {
        for (y, row) in img.rows_mut().enumerate() {
            for (x, p) in row.enumerate() {
                let bytes: [u8; 4] = data[y * stride + x * 4..][..4].try_into().unwrap();
                p.0 = unpremultiply(u32::from_ne_bytes(bytes));
            }
        }
    }
}

fn draw_on_image(img: &mut RgbaImage, marks: &[Mark]) {
    if marks.iter().all(|m| matches!(m, Mark::Pixelate { .. } | Mark::Blur { .. })) {
        return;
    }
    let (w, h) = img.dimensions();
    let Some(mut surface) = surface_from_image(img) else { return };
    {
        let Ok(cr) = Context::new(&surface) else { return };
        draw_marks(&cr, w as f64, h as f64, marks);
    }
    copy_surface_into(&mut surface, img);
}

fn premultiply([r, g, b, a]: [u8; 4]) -> u32 {
    let m = |c: u8| (c as u32 * a as u32 + 127) / 255;
    ((a as u32) << 24) | (m(r) << 16) | (m(g) << 8) | m(b)
}

fn unpremultiply(px: u32) -> [u8; 4] {
    let a = (px >> 24) & 0xff;
    let (r, g, b) = ((px >> 16) & 0xff, (px >> 8) & 0xff, px & 0xff);
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let un = |c: u32| ((c * 255 + a / 2) / a).min(255) as u8;
    [un(r), un(g), un(b), a as u8]
}

/// Downscale for the live canvas; copy/save still use the full picture.
pub fn preview(src: &RgbaImage, max_edge: u32) -> RgbaImage {
    let (w, h) = src.dimensions();
    if w.max(h) <= max_edge {
        return src.clone();
    }
    let k = max_edge as f64 / w.max(h) as f64;
    imageops::resize(src, ((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1), FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annot::Mark;
    use image::Rgba;

    const RED: Colour = Colour::rgb(1.0, 0.0, 0.0);

    fn sample() -> RgbaImage {
        let mut img = RgbaImage::from_pixel(40, 20, Rgba([100, 100, 100, 255]));
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        img.put_pixel(39, 0, Rgba([0, 0, 255, 255]));
        img
    }

    #[test]
    fn render_leaves_the_source_alone() {
        let src = sample();
        let before = src.clone();
        let mut st = EditState::default();
        st.marks.push(Mark::Rect { rect: NormRect::new(0.2, 0.2, 0.5, 0.5), colour: RED, size: 0.04 });
        let _ = render(&src, &st);
        assert_eq!(src, before);
    }

    #[test]
    fn a_rectangle_is_drawn_where_asked() {
        let mut st = EditState::default();
        st.marks.push(Mark::Rect { rect: NormRect::new(0.2, 0.2, 0.6, 0.6), colour: RED, size: 0.04 });
        let img = render(&RgbaImage::from_pixel(100, 100, Rgba([10, 20, 30, 255])), &st);
        assert_eq!(img.get_pixel(20, 50).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(50, 50).0, [10, 20, 30, 255]);
        assert_eq!(img.get_pixel(2, 2).0, [10, 20, 30, 255]);
    }

    #[test]
    fn crop_takes_the_asked_part() {
        let mut st = EditState::default();
        st.set_crop(Some(NormRect::new(0.5, 0.0, 0.5, 0.5)));
        let out = render(&sample(), &st);
        assert_eq!(out.dimensions(), (20, 10));
        assert_eq!(out.get_pixel(19, 0).0, [0, 0, 255, 255]);
    }

    #[test]
    fn pixelate_hides_the_region() {
        let mut src = RgbaImage::from_pixel(40, 40, Rgba([10, 10, 10, 255]));
        src.put_pixel(4, 4, Rgba([250, 0, 0, 255]));
        src.put_pixel(5, 5, Rgba([0, 250, 0, 255]));
        let mut st = EditState::default();
        st.marks.push(Mark::Pixelate { rect: NormRect::new(0.0, 0.0, 0.5, 0.5), size: 0.2 });
        let out = render(&src, &st);
        assert_eq!(out.get_pixel(4, 4), out.get_pixel(5, 5));
        assert_eq!(src.get_pixel(4, 4).0, [250, 0, 0, 255]);
    }

    #[test]
    fn a_flattened_copy_leaves_the_original_file_unchanged() {
        let dir = std::env::temp_dir().join("nixe-capture-annot-file");
        let _ = std::fs::create_dir_all(&dir);
        let src_path = dir.join("orig.png");
        sample().save(&src_path).unwrap();
        let before = std::fs::read(&src_path).unwrap();
        let mut st = EditState::default();
        st.marks.push(Mark::Rect { rect: NormRect::new(0.1, 0.1, 0.5, 0.5), colour: RED, size: 0.05 });
        let src = image::open(&src_path).unwrap().to_rgba8();
        let out = render(&src, &st);
        let dest = dir.join("flat.png");
        out.save(&dest).unwrap();
        assert_eq!(std::fs::read(&src_path).unwrap(), before);
        assert_ne!(std::fs::read(&dest).unwrap(), before);
    }
}
