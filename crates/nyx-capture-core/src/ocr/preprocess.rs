//! Grayscale, upsample small crops, slight contrast stretch. No binarize.

use image::{DynamicImage, GrayImage, imageops::FilterType};

const MIN_SHORT: u32 = 400;

pub struct Prep {
    pub image: DynamicImage,
    /// Multiply preprocessed boxes by these to recover original pixels.
    pub sx: f32,
    pub sy: f32,
}

pub fn prepare(img: &DynamicImage) -> Prep {
    let gray = stretch(&img.to_luma8());
    let (ow, oh) = gray.dimensions();
    let short = ow.min(oh);
    let work = if short > 0 && short < MIN_SHORT {
        let k = MIN_SHORT as f32 / short as f32;
        let nw = ((ow as f32) * k).round().max(1.0) as u32;
        let nh = ((oh as f32) * k).round().max(1.0) as u32;
        image::imageops::resize(&gray, nw, nh, FilterType::CatmullRom)
    } else {
        gray
    };
    let (pw, ph) = work.dimensions();
    Prep {
        image: DynamicImage::ImageLuma8(work),
        sx: if pw == 0 { 1.0 } else { ow as f32 / pw as f32 },
        sy: if ph == 0 { 1.0 } else { oh as f32 / ph as f32 },
    }
}

fn stretch(gray: &GrayImage) -> GrayImage {
    let n = (gray.width() as u64) * (gray.height() as u64);
    if n == 0 {
        return gray.clone();
    }
    let mut hist = [0u64; 256];
    for p in gray.pixels() {
        hist[p.0[0] as usize] += 1;
    }
    let lo_t = ((n as f64) * 0.02) as u64;
    let hi_t = ((n as f64) * 0.98) as u64;
    let mut acc = 0u64;
    let mut lo = 0u8;
    let mut hi = 255u8;
    let mut lo_set = false;
    for (i, c) in hist.iter().enumerate() {
        acc += *c;
        if !lo_set && acc >= lo_t {
            lo = i as u8;
            lo_set = true;
        }
        if acc >= hi_t {
            hi = i as u8;
            break;
        }
    }
    if hi <= lo {
        return gray.clone();
    }
    let span = (hi - lo) as f32;
    let mut out = gray.clone();
    for p in out.pixels_mut() {
        let t = ((p.0[0].saturating_sub(lo) as f32 / span) * 255.0).round().clamp(0.0, 255.0) as u8;
        p.0[0] = t;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn small_images_are_upsampled() {
        let img = DynamicImage::ImageLuma8(GrayImage::from_pixel(80, 40, Luma([80])));
        let p = prepare(&img);
        assert!(p.image.width().min(p.image.height()) >= MIN_SHORT);
        assert!(p.sx < 1.0);
        assert!(p.sy < 1.0);
    }

    #[test]
    fn large_images_keep_size() {
        let img = DynamicImage::ImageLuma8(GrayImage::from_pixel(640, 480, Luma([90])));
        let p = prepare(&img);
        assert_eq!(p.image.width(), 640);
        assert_eq!(p.sx, 1.0);
        assert_eq!(p.sy, 1.0);
    }
}
