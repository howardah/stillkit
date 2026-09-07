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
    let mut built_in_attempted = false;
    if matches!(options.pipeline, Pipeline::Sips) {
        return raw::load_with_sips(&path, size, full, report_tool);
    }
    if matches!(options.pipeline, Pipeline::Magick) {
        return raw::load_with_magick("magick", &path, size, full, report_tool);
    }
    if options.pipeline.allows_tools() {
        // Embedded decoding is substantially faster than launching an external
        // decoder. Do not eagerly decode the full primary image here: sips is
        // faster for that work on the benchmark machine.
        if let Some(image) = load_embedded(&path, options) {
            return Ok(image);
        }
        #[cfg(target_os = "macos")]
        match raw::with_sips(&path, size, full, report_tool) {
            Ok(image) => return Ok(image),
            Err(error) => errors.push(error),
        }
        // The corrected built-in HEIC decoder beats ImageMagick for primary
        // images. ImageMagick remains a fallback for unsupported HEIC variants.
        if crate::shared::image::is_heic_family(&path) {
            built_in_attempted = true;
            match load_builtin_primary(&path, options) {
                Ok(image) => return Ok(image),
                Err(error) => errors.push(error),
            }
        }
        for program in ["magick", "convert"] {
            match raw::load_with_magick(program, &path, size, full, report_tool) {
                Ok(image) => return Ok(image),
                Err(error) => errors.push(error),
            }
        }
    }
    if built_in_attempted {
        return Err(preview_error(&path, &errors));
    }
    let options = if matches!(options.pipeline, Pipeline::Auto) {
        PreviewOptions {
            use_embedded: false,
            ..options
        }
    } else {
        options
    };
    match load_builtin(&path, options) {
        Ok(image) => Ok(image),
        Err(error) => {
            errors.push(error);
            Err(preview_error(&path, &errors))
        }
    }
}

fn preview_error(path: &Path, errors: &[String]) -> String {
    format!(
        "Failed to generate preview for {}: {}",
        path.display(),
        errors.join("; ")
    )
}

fn load_embedded(path: &Path, options: PreviewOptions) -> Option<DynamicImage> {
    if !options.use_embedded || options.full {
        return None;
    }
    if raw::is_raw(path) {
        return raw::embedded_preview(path, options.max_dimension).ok();
    }
    let data = fs::read(path).ok()?;
    super::embedded::decode(&data, options.max_dimension)
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

fn load_builtin_primary(path: &Path, options: PreviewOptions) -> Result<DynamicImage, String> {
    load_builtin(
        path,
        PreviewOptions {
            use_embedded: false,
            ..options
        },
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::previews::OutputFormat;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn automatic_mode_uses_embedded_heic_before_external_tools() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/DSCF0656.HEIC");
        let tool_reported = AtomicBool::new(false);
        let image = load_preview(
            &path,
            PreviewOptions {
                max_dimension: 1000,
                format: OutputFormat::Jpeg,
                full: false,
                use_embedded: true,
                clear_metadata: false,
                quality: 75,
                pipeline: Pipeline::Auto,
            },
            &|_| tool_reported.store(true, Ordering::Relaxed),
        )
        .expect("embedded HEIC preview should decode");

        assert_eq!(image.width(), 1920);
        assert!(!tool_reported.load(Ordering::Relaxed));
    }
}
