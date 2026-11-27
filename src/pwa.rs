//! Generated PWA icons (cat mark), mirroring `generate_icon` in the Python build.

use std::path::Path;

use anyhow::Result;
use image::{ImageFormat, Rgba, RgbaImage};

/// Draw a simple cat-face icon at the given square size.
pub fn generate_icon(path: &Path, size: u32, accent: &str, maskable: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (r, g, b) = crate::theme::hex_to_rgb(accent).unwrap_or((240, 160, 106));
    let fill = Rgba([r, g, b, 255]);
    let bg = Rgba([30u8, 25, 23, 255]);
    let mut img = RgbaImage::from_pixel(size, size, bg);

    // Rounded-rectangle background for non-maskable icons.
    if !maskable {
        rounded_rect(&mut img, 0, 0, size, size, (size as f32 * 0.22) as u32, bg);
    }

    // Head (circle-ish ellipse by rows) plus ears.
    let pad = (size as f32 * if maskable { 0.18 } else { 0.14 }) as u32;
    let left = pad;
    let right = size - pad;
    let top = pad + ((size - pad * 2) as f32 * 0.22) as u32;
    let bottom = size - pad - ((size - pad * 2) as f32 * 0.04) as u32;
    ellipse(&mut img, left, top, right, bottom, fill, bg);
    for (x0, x1) in [
        (left, left + (right - left) / 3),
        (right - (right - left) / 3, right),
    ] {
        let ear_top = pad + ((size - pad * 2) as f32 * 0.10) as u32;
        triangle(
            &mut img,
            x0,
            ear_top,
            x1,
            ear_top + (bottom - top) / 3,
            x1,
            ear_top,
            fill,
            bg,
        );
    }

    // Eyes.
    let eye_r = ((size as f32 * 0.035) as u32).max(2);
    let eye_y = (size as f32 * 0.50) as u32;
    for cx in [(size as f32 * 0.38) as u32, (size as f32 * 0.62) as u32] {
        fill_circle(&mut img, cx, eye_y, eye_r, bg);
    }

    img.save(path)?;
    // Re-save through explicit format to keep determinism on all platforms.
    let _ = ImageFormat::Png;
    Ok(())
}

fn put(img: &mut RgbaImage, x: i64, y: i64, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, color);
    }
}

fn fill_circle(img: &mut RgbaImage, cx: u32, cy: u32, r: u32, color: Rgba<u8>) {
    let r2 = (r * r) as i64;
    for dy in -(r as i64)..=(r as i64) {
        for dx in -(r as i64)..=(r as i64) {
            if dx * dx + dy * dy <= r2 {
                put(img, cx as i64 + dx, cy as i64 + dy, color);
            }
        }
    }
}

fn ellipse(
    img: &mut RgbaImage,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    _fill: Rgba<u8>,
    color: Rgba<u8>,
) {
    let cx = (x0 as f32 + x1 as f32) / 2.0;
    let cy = (y0 as f32 + y1 as f32) / 2.0;
    let rx = (x1 as f32 - x0 as f32) / 2.0;
    let ry = (y1 as f32 - y0 as f32) / 2.0;
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = (x as f32 - cx) / rx.max(1.0);
            let dy = (y as f32 - cy) / ry.max(1.0);
            if dx * dx + dy * dy <= 1.0 {
                put(img, x as i64, y as i64, color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn triangle(
    img: &mut RgbaImage,
    ax: u32,
    ay: u32,
    bx: u32,
    by: u32,
    cx: u32,
    cy: u32,
    _fill: Rgba<u8>,
    color: Rgba<u8>,
) {
    let min_x = ax.min(bx).min(cx) as i64;
    let max_x = ax.max(bx).max(cx) as i64;
    let min_y = ay.min(by).min(cy) as i64;
    let max_y = ay.max(by).max(cy) as i64;
    let sign = |px: i64, py: i64, qx: i64, qy: i64, rx: i64, ry: i64| {
        (px - rx) * (qy - ry) - (qx - rx) * (py - ry)
    };
    let (ax, ay, bx, by, cx, cy) = (
        ax as i64, ay as i64, bx as i64, by as i64, cx as i64, cy as i64,
    );
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let d1 = sign(x, y, ax, ay, bx, by);
            let d2 = sign(x, y, bx, by, cx, cy);
            let d3 = sign(x, y, cx, cy, ax, ay);
            let has_neg = d1 < 0 || d2 < 0 || d3 < 0;
            let has_pos = d1 > 0 || d2 > 0 || d3 > 0;
            if !(has_neg && has_pos) {
                put(img, x, y, color);
            }
        }
    }
}

fn rounded_rect(img: &mut RgbaImage, x0: u32, y0: u32, w: u32, h: u32, r: u32, color: Rgba<u8>) {
    let r2 = (r * r) as i64;
    for y in y0..(y0 + h) {
        for x in x0..(x0 + w) {
            let dx = if x < x0 + r {
                (x0 + r - x) as i64
            } else if x > x0 + w - r {
                (x - (x0 + w - r)) as i64
            } else {
                0
            };
            let dy = if y < y0 + r {
                (y0 + r - y) as i64
            } else if y > y0 + h - r {
                (y - (y0 + h - r)) as i64
            } else {
                0
            };
            if dx == 0 || dy == 0 || dx * dx + dy * dy <= r2 {
                put(img, x as i64, y as i64, color);
            }
        }
    }
}
