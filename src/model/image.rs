use base64::{Engine as _, engine::general_purpose};
use image::{
    ColorType, ImageBuffer, ImageEncoder, Rgba, codecs::png::PngEncoder, imageops::FilterType,
};
use std::sync::Arc;

const IMAGE_PREVIEW_MAX_WIDTH: usize = 1440;
const IMAGE_PREVIEW_MAX_HEIGHT: usize = 440;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone, Debug)]
pub struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    pub bytes: Option<Arc<Vec<u8>>>,
    pub preview_url: Option<String>,
}

impl PartialEq for ClipboardImage {
    fn eq(&self, other: &Self) -> bool {
        if self.width != other.width || self.height != other.height {
            return false;
        }

        match (&self.bytes, &other.bytes) {
            (Some(left), Some(right)) => left == right,
            _ => self.preview_url.is_some() && self.preview_url == other.preview_url,
        }
    }
}

impl Eq for ClipboardImage {}

impl ClipboardImage {
    pub fn from_rgba(width: usize, height: usize, bytes: Vec<u8>) -> Self {
        let preview_url = encode_image_preview(&bytes, width, height).map(|png| {
            format!(
                "data:image/png;base64,{}",
                general_purpose::STANDARD.encode(png)
            )
        });

        Self {
            width,
            height,
            bytes: Some(Arc::new(bytes)),
            preview_url,
        }
    }

    pub fn from_stored_bytes(
        width: usize,
        height: usize,
        bytes: Vec<u8>,
        preview_url: Option<String>,
    ) -> Option<Self> {
        if bytes.starts_with(PNG_SIGNATURE) {
            let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
            let (width, height) = image.dimensions();

            return Some(Self {
                width: width as usize,
                height: height as usize,
                bytes: Some(Arc::new(image.into_raw())),
                preview_url,
            });
        }

        let expected_len = width.checked_mul(height)?.checked_mul(4)?;
        (bytes.len() == expected_len).then(|| Self {
            width,
            height,
            bytes: Some(Arc::new(bytes)),
            preview_url,
        })
    }

    pub fn has_bytes(&self) -> bool {
        self.bytes.is_some()
    }

    pub fn rgba_bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref().map(Vec::as_slice)
    }

    pub fn to_png_bytes(&self) -> Option<Vec<u8>> {
        encode_png(self.rgba_bytes()?, self.width, self.height)
    }

    pub fn preview_png_bytes(&self) -> Option<Vec<u8>> {
        encode_image_preview(self.rgba_bytes()?, self.width, self.height)
    }

    pub fn stored_bytes(&self) -> Option<Vec<u8>> {
        self.to_png_bytes()
            .or_else(|| self.rgba_bytes().map(Vec::from))
    }
}

fn encode_png(bytes: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    if bytes.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }

    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(bytes, width as u32, height as u32, ColorType::Rgba8.into())
        .ok()?;

    Some(png)
}

fn encode_image_preview(bytes: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    let (preview_width, preview_height) = image_preview_dimensions(width, height)?;
    if preview_width == width && preview_height == height {
        return encode_png(bytes, width, height);
    }

    let preview = resize_rgba_high_quality(bytes, width, height, preview_width, preview_height)?;
    encode_png(&preview, preview_width, preview_height)
}

fn image_preview_dimensions(width: usize, height: usize) -> Option<(usize, usize)> {
    if width == 0 || height == 0 {
        return None;
    }

    let width_scale = IMAGE_PREVIEW_MAX_WIDTH as f64 / width as f64;
    let height_scale = IMAGE_PREVIEW_MAX_HEIGHT as f64 / height as f64;
    let scale = width_scale.min(height_scale).min(1.0);

    Some((
        ((width as f64 * scale).round() as usize).max(1),
        ((height as f64 * scale).round() as usize).max(1),
    ))
}

fn resize_rgba_high_quality(
    bytes: &[u8],
    width: usize,
    height: usize,
    target_width: usize,
    target_height: usize,
) -> Option<Vec<u8>> {
    if bytes.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }

    let source =
        ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width as u32, height as u32, bytes.to_vec())?;
    let resized = image::imageops::resize(
        &source,
        target_width as u32,
        target_height as u32,
        FilterType::Lanczos3,
    );

    Some(resized.into_raw())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_preview_dimensions_preserve_aspect_ratio_without_upscaling() {
        assert_eq!(image_preview_dimensions(1920, 1080), Some((782, 440)));
        assert_eq!(image_preview_dimensions(2000, 100), Some((1440, 72)));
        assert_eq!(image_preview_dimensions(32, 16), Some((32, 16)));
        assert_eq!(image_preview_dimensions(0, 16), None);
    }

    #[test]
    fn metadata_only_image_matches_full_image_by_preview() {
        let full = ClipboardImage {
            width: 2,
            height: 1,
            bytes: Some(Arc::new(vec![255, 0, 0, 255, 0, 255, 0, 255])),
            preview_url: Some("preview".to_string()),
        };
        let metadata_only = ClipboardImage {
            width: 2,
            height: 1,
            bytes: None,
            preview_url: Some("preview".to_string()),
        };

        assert_eq!(metadata_only, full);
    }
}
