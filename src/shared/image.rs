use image::DynamicImage;
use std::{fs, path::Path};

pub(crate) fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg"
                    | "jpeg"
                    | "png"
                    | "webp"
                    | "gif"
                    | "bmp"
                    | "tiff"
                    | "tif"
                    | "heic"
                    | "heif"
                    | "hif"
                    | "arw"
                    | "cr2"
                    | "nef"
                    | "raf"
                    | "dng"
            )
        })
}

pub(crate) fn is_heic_family(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "heic" | "heif" | "hif"
            )
        })
}

pub(crate) fn load_image(path: &Path) -> Result<DynamicImage, String> {
    if is_heic_family(path) {
        load_heic_image(path)
    } else {
        image::ImageReader::open(path)
            .map_err(|error| format!("Failed to open image {}: {error}", path.display()))?
            .decode()
            .map_err(|error| format!("Failed to decode image {}: {error}", path.display()))
    }
}

fn load_heic_image(path: &Path) -> Result<DynamicImage, String> {
    use heic::{DecoderConfig, PixelLayout};

    let data = fs::read(path)
        .map_err(|error| format!("Failed to read HEIC file {}: {error}", path.display()))?;
    let output = DecoderConfig::new()
        .decode(&data, PixelLayout::Rgba8)
        .map_err(|error| format!("Failed to decode HEIC image {}: {error}", path.display()))?;

    let expected_bytes = output.width as usize * output.height as usize * 4;
    let actual_bytes = output.data.len();
    let image =
        image::RgbaImage::from_raw(output.width, output.height, output.data).ok_or_else(|| {
            format!(
                "Failed to create image from HEIC data for {} (expected {expected_bytes} bytes, \
                 got {actual_bytes} bytes)",
                path.display()
            )
        })?;

    Ok(DynamicImage::ImageRgba8(image))
}
