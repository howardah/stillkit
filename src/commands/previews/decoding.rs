use super::image::PreviewOptions;
use crate::shared::{Pipeline, ToolReporter, decoding as raw};
use image::DynamicImage;
use std::{fs, path::Path};

pub(super) fn load_preview(
    path: &Path,
    options: PreviewOptions,
    report_tool: ToolReporter<'_>,
) -> Result<DynamicImage, String> {
    let path = fs::canonicalize(path)
        .map_err(|e| format!("Failed to resolve image input {}: {e}", path.display()))?;
    let size = options.max_dimension;
    let full = options.full;
    let mut errors = Vec::new();
    if matches!(options.pipeline, Pipeline::Sips) {
        return raw::load_with_sips(&path, size, full, report_tool);
    }
    if matches!(options.pipeline, Pipeline::Magick) {
        return raw::load_with_magick("magick", &path, size, full, report_tool);
    }
    if options.pipeline.allows_tools() {
        #[cfg(target_os = "macos")]
        match raw::with_sips(&path, size, full, report_tool) {
            Ok(image) => return Ok(image),
            Err(error) => errors.push(error),
        }
        for program in ["magick", "convert"] {
            match raw::load_with_magick(program, &path, size, full, report_tool) {
                Ok(image) => return Ok(image),
                Err(error) => errors.push(error),
            }
        }
    }
    match load_builtin(&path, options) {
        Ok(image) => Ok(image),
        Err(error) => {
            errors.push(error);
            Err(format!(
                "Failed to generate preview for {}: {}",
                path.display(),
                errors.join("; ")
            ))
        }
    }
}

fn load_builtin(path: &Path, options: PreviewOptions) -> Result<DynamicImage, String> {
    let use_embedded = options.use_embedded && !options.full;
    if raw::is_raw(path) {
        if use_embedded && let Ok(image) = raw::embedded_preview(path, options.max_dimension) {
            return Ok(image);
        }
        return raw::develop_raw(path);
    }
    let data =
        fs::read(path).map_err(|e| format!("Failed to read HEIC image {}: {e}", path.display()))?;
    if use_embedded && let Some(image) = super::embedded::decode(&data, options.max_dimension) {
        return Ok(image);
    }
    decode_heic_rgb(&data, path)
}

// Previews deliberately discard alpha. Request RGB directly instead of allocating
// a full RGBA frame and then copying it to RGB; exposure retains the RGBA loader.
fn decode_heic_rgb(data: &[u8], path: &Path) -> Result<DynamicImage, String> {
    let output = heic::DecoderConfig::new()
        .decode(data, heic::PixelLayout::Rgb8)
        .map_err(|error| format!("Failed to decode HEIC image {}: {error}", path.display()))?;
    image::RgbImage::from_raw(output.width, output.height, output.data)
        .map(DynamicImage::ImageRgb8)
        .ok_or_else(|| format!("Invalid RGB pixels from HEIC image {}", path.display()))
}
