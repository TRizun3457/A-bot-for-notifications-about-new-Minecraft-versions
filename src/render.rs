//! Превью скина и плаща, полностью локально: никаких сторонних сервисов рендера.
//!
//! * лицо игрока: область 8×8 в точке (8, 8) и второй слой («шляпа») в точке (40, 8);
//! * плащ: внешняя сторона 10×16 в точке (1, 1) на текстуре 64×32.
//!
//! Для HD-текстур координаты умножаются на `ширина / 64`.

use anyhow::{bail, Context, Result};
use image::{imageops, DynamicImage, ImageFormat, RgbaImage};
use std::io::Cursor;

const MAX_SIDE: u32 = 4096;

fn load(png: &[u8]) -> Result<RgbaImage> {
    let img = image::load_from_memory_with_format(png, ImageFormat::Png).context("not a valid PNG")?;
    let rgba = img.to_rgba8();
    if rgba.width() < 64 || rgba.height() < 32 || rgba.width() > MAX_SIDE || rgba.height() > MAX_SIDE {
        bail!("unexpected texture size {}x{}", rgba.width(), rgba.height());
    }
    Ok(rgba)
}

fn encode(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img.clone()).write_to(&mut out, ImageFormat::Png)?;
    Ok(out.into_inner())
}

/// Лицо игрока 256×256 с учётом второго слоя.
pub fn face_preview(skin_png: &[u8]) -> Result<Vec<u8>> {
    let skin = load(skin_png)?;
    let s = skin.width() / 64;
    let mut face = imageops::crop_imm(&skin, 8 * s, 8 * s, 8 * s, 8 * s).to_image();
    let hat = imageops::crop_imm(&skin, 40 * s, 8 * s, 8 * s, 8 * s).to_image();
    imageops::overlay(&mut face, &hat, 0, 0);
    let big = imageops::resize(&face, 256, 256, imageops::FilterType::Nearest);
    encode(&big)
}

/// Внешняя сторона плаща 160×256.
pub fn cape_preview(cape_png: &[u8]) -> Result<Vec<u8>> {
    let cape = load(cape_png)?;
    let s = cape.width() / 64;
    let front = imageops::crop_imm(&cape, s, s, 10 * s, 16 * s).to_image();
    let big = imageops::resize(&front, 160, 256, imageops::FilterType::Nearest);
    encode(&big)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn png_of(w: u32, h: u32, f: impl Fn(u32, u32) -> Rgba<u8>) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, f);
        encode(&img).unwrap()
    }

    #[test]
    fn face_uses_head_region_and_overlays_hat() {
        // Голова красная, второй слой синий только в левом верхнем пикселе.
        let skin = png_of(64, 64, |x, y| {
            if (40..48).contains(&x) && (8..16).contains(&y) {
                if x == 40 && y == 8 {
                    Rgba([0, 0, 255, 255])
                } else {
                    Rgba([0, 0, 0, 0])
                }
            } else if (8..16).contains(&x) && (8..16).contains(&y) {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 255, 0, 255])
            }
        });
        let out = image::load_from_memory(&face_preview(&skin).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!((out.width(), out.height()), (256, 256));
        assert_eq!(out.get_pixel(5, 5), &Rgba([0, 0, 255, 255]), "шляпа поверх головы");
        assert_eq!(
            out.get_pixel(200, 200),
            &Rgba([255, 0, 0, 255]),
            "остальное лицо красное"
        );
    }

    #[test]
    fn hd_skin_scales_coordinates() {
        let skin = png_of(128, 128, |x, y| {
            if (16..32).contains(&x) && (16..32).contains(&y) {
                Rgba([10, 20, 30, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        let out = image::load_from_memory(&face_preview(&skin).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(out.get_pixel(128, 128), &Rgba([10, 20, 30, 255]));
    }

    #[test]
    fn cape_front_is_cropped_and_scaled() {
        let cape = png_of(64, 32, |x, y| {
            if (1..11).contains(&x) && (1..17).contains(&y) {
                Rgba([200, 0, 0, 255])
            } else {
                Rgba([0, 0, 200, 255])
            }
        });
        let out = image::load_from_memory(&cape_preview(&cape).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!((out.width(), out.height()), (160, 256));
        assert_eq!(out.get_pixel(0, 0), &Rgba([200, 0, 0, 255]));
        assert_eq!(out.get_pixel(159, 255), &Rgba([200, 0, 0, 255]));
    }

    #[test]
    fn rejects_garbage_and_wrong_sizes() {
        assert!(face_preview(b"not a png").is_err());
        let tiny = png_of(16, 16, |_, _| Rgba([0, 0, 0, 255]));
        assert!(face_preview(&tiny).is_err());
    }
}
