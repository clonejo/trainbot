//! Pixel-format conversion helpers.

use image::{Rgba, RgbaImage};

#[inline]
fn ycbcr_to_rgb(y: i32, cb: i32, cr: i32) -> [u8; 3] {
    // ITU-R BT.601
    let r = (y + (1402 * cr / 1000)).clamp(0, 255) as u8;
    let g = (y - (344136 * cb / 1000000) - (714136 * cr / 1000000)).clamp(0, 255) as u8;
    let b = (y + (1772 * cb / 1000)).clamp(0, 255) as u8;
    [r, g, b]
}

/// YUYV 4:2:2 packed → RGBA.
/// Input: `width * height * 2` bytes in [Y0 Cb Y1 Cr] groups of 4 bytes / 2 pixels.
pub fn yuyv_to_rgba(data: &[u8], width: u32, height: u32) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        for xp in 0..width / 2 {
            let base = ((y * width + xp * 2) * 2) as usize;
            let y0 = data[base] as i32;
            let cb = data[base + 1] as i32 - 128;
            let y1 = data[base + 2] as i32;
            let cr = data[base + 3] as i32 - 128;
            let [r0, g0, b0] = ycbcr_to_rgb(y0, cb, cr);
            let [r1, g1, b1] = ycbcr_to_rgb(y1, cb, cr);
            img.put_pixel(xp * 2, y, Rgba([r0, g0, b0, 255]));
            img.put_pixel(xp * 2 + 1, y, Rgba([r1, g1, b1, 255]));
        }
    }
    img
}

/// YUV 4:2:0 planar (YU12 / I420) → RGBA.
/// Layout: Y plane (w*h), Cb plane (w/2 * h/2), Cr plane (w/2 * h/2).
pub fn yuv420_to_rgba(data: &[u8], width: u32, height: u32) -> RgbaImage {
    let y_size = (width * height) as usize;
    let uv_size = y_size / 4;
    let y_plane = &data[..y_size];
    let cb_plane = &data[y_size..y_size + uv_size];
    let cr_plane = &data[y_size + uv_size..y_size + 2 * uv_size];

    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let luma = y_plane[(y * width + x) as usize] as i32;
            let uv_idx = ((y / 2) * (width / 2) + x / 2) as usize;
            let cb = cb_plane[uv_idx] as i32 - 128;
            let cr = cr_plane[uv_idx] as i32 - 128;
            let [r, g, b] = ycbcr_to_rgb(luma, cb, cr);
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    img
}
