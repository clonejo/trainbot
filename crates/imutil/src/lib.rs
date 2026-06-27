use image::{DynamicImage, GrayImage, ImageFormat, RgbaImage};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unknown image extension: {0}")]
    UnknownExtension(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn load(path: impl AsRef<Path>) -> Result<DynamicImage> {
    Ok(image::open(path)?)
}

pub fn save(path: impl AsRef<Path>, img: &DynamicImage) -> Result<()> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let fmt = match ext.as_str() {
        "png" => ImageFormat::Png,
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        other => return Err(Error::UnknownExtension(other.to_string())),
    };
    img.save_with_format(path, fmt)?;
    Ok(())
}

pub fn save_jpeg(path: impl AsRef<Path>, img: &DynamicImage, quality: u8) -> Result<()> {
    use image::codecs::jpeg::JpegEncoder;
    use std::fs::File;
    use std::io::BufWriter;
    let f = File::create(path)?;
    let mut enc = JpegEncoder::new_with_quality(BufWriter::new(f), quality);
    enc.encode_image(img)?;
    Ok(())
}

pub fn to_rgba(img: DynamicImage) -> RgbaImage {
    img.into_rgba8()
}

pub fn to_gray(img: DynamicImage) -> GrayImage {
    img.into_luma8()
}

pub fn sub(img: &DynamicImage, x: u32, y: u32, w: u32, h: u32) -> DynamicImage {
    img.crop_imm(x, y, w, h)
}

pub fn resize(img: &DynamicImage, w: u32, h: u32) -> DynamicImage {
    img.resize_exact(w, h, image::imageops::FilterType::Lanczos3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    fn rand_rgba(seed: u64, w: u32, h: u32) -> DynamicImage {
        let mut pix = vec![0u8; (w * h * 4) as usize];
        let mut s = seed;
        for b in pix.iter_mut() {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *b = (s >> 56) as u8;
        }
        DynamicImage::ImageRgba8(RgbaImage::from_raw(w, h, pix).unwrap())
    }

    #[test]
    fn roundtrip_png() {
        let dir = std::env::temp_dir();
        let path = dir.join("imutil_test.png");
        let img = rand_rgba(123, 64, 32);
        save(&path, &img).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.width(), 64);
        assert_eq!(loaded.height(), 32);
    }

    #[test]
    fn roundtrip_jpeg() {
        let dir = std::env::temp_dir();
        let path = dir.join("imutil_test.jpg");
        let img = rand_rgba(123, 64, 32);
        save(&path, &img).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.width(), 64);
        assert_eq!(loaded.height(), 32);
    }

    #[test]
    fn sub_image() {
        let img = rand_rgba(42, 100, 80);
        let cropped = sub(&img, 10, 5, 30, 20);
        assert_eq!(cropped.width(), 30);
        assert_eq!(cropped.height(), 20);
    }

    #[test]
    fn to_rgba_gray_roundtrip() {
        let img = rand_rgba(7, 16, 16);
        let rgba = to_rgba(img.clone());
        assert_eq!(rgba.width(), 16);
        let gray = to_gray(img);
        assert_eq!(gray.width(), 16);
    }

    #[test]
    fn unknown_extension_errors() {
        let dir = std::env::temp_dir();
        let img = rand_rgba(1, 4, 4);
        assert!(save(dir.join("x.bmp"), &img).is_err());
    }
}
