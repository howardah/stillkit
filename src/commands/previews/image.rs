use super::OutputFormat;
use crate::shared::Pipeline;
use crate::shared::image::{is_heic_family, load_image};
use image::{DynamicImage, ImageEncoder};
#[cfg(all(test, feature = "native-heic"))]
use std::fs;
#[cfg(test)]
use std::process::Command as ProcessCommand;
use std::{io::Cursor, path::Path};

#[derive(Clone, Copy)]
pub(crate) struct PreviewOptions {
    pub max_dimension: u32,
    pub format: OutputFormat,
    pub full: bool,
    pub use_embedded: bool,
    pub clear_metadata: bool,
    pub quality: u8,
    pub lossless: bool,
    pub pipeline: Pipeline,
}

struct GeneratedPreview {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    orientation_normalized: bool,
    source_bytes: Option<Vec<u8>>,
}

pub(crate) fn generate_preview(
    input_path: &Path,
    output_path: &Path,
    options: PreviewOptions,
    report_tool: crate::shared::ToolReporter<'_>,
) -> Result<(), String> {
    let preview = generate_preview_image(input_path, options, report_tool)?;
    if options.clear_metadata {
        write_preview(output_path, &preview.bytes)?;
    } else {
        crate::shared::metadata::write_with_metadata(
            input_path,
            output_path,
            crate::shared::metadata::EncodedImage {
                bytes: preview.bytes,
                width: preview.width,
                height: preview.height,
            },
            preview.source_bytes.as_deref(),
            preview.orientation_normalized,
            options.pipeline,
            report_tool,
        )?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn do_generate_preview(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    format: OutputFormat,
    full: bool,
    clear_metadata: bool,
    quality: u8,
) -> Result<(), String> {
    generate_preview(
        input_path,
        output_path,
        PreviewOptions {
            max_dimension,
            format,
            full,
            use_embedded: true,
            clear_metadata,
            quality,
            lossless: false,
            pipeline: Pipeline::Auto,
        },
        &|_| {},
    )
}

fn generate_preview_image(
    input_path: &Path,
    options: PreviewOptions,
    report_tool: crate::shared::ToolReporter<'_>,
) -> Result<GeneratedPreview, String> {
    let normalized = super::raw::is_raw(input_path) || is_heic_family(input_path);
    let (img, source_bytes) = if normalized {
        let decoded = super::decoding::load_preview(input_path, options, report_tool)?;
        (decoded.image.into_rgb8().into(), decoded.source_bytes)
    } else {
        (load_image(input_path)?, None)
    };

    let img = if matches!(options.format, OutputFormat::Jxl) {
        crate::shared::jxl::normalize(img, input_path)?
    } else {
        img
    };
    let resized = super::resize::preview(img, options.max_dimension, options.full);

    let bytes = if matches!(options.format, OutputFormat::Jxl) {
        crate::shared::jxl::encode(
            &resized,
            crate::shared::jxl::Encoding {
                quality: options.quality,
                lossless: options.lossless,
            },
        )?
    } else {
        encode_image(&resized, options.format, options.quality)
            .map_err(|e| format!("Failed to encode image {}: {}", input_path.display(), e))?
    };
    Ok(GeneratedPreview {
        bytes,
        width: resized.width(),
        height: resized.height(),
        orientation_normalized: normalized || crate::shared::jxl::is_jxl(input_path),
        source_bytes,
    })
}

fn write_preview(output_path: &Path, bytes: &[u8]) -> Result<(), String> {
    crate::shared::metadata::write_atomically(output_path, bytes)
}

pub(super) fn encode_image(
    image: &DynamicImage,
    format: OutputFormat,
    quality: u8,
) -> image::ImageResult<Vec<u8>> {
    let mut bytes = Vec::new();
    match format {
        OutputFormat::Jxl => {
            return crate::shared::jxl::encode(
                image,
                crate::shared::jxl::Encoding {
                    quality,
                    lossless: false,
                },
            )
            .map_err(|e| image::ImageError::IoError(std::io::Error::other(e)));
        }
        OutputFormat::Jpeg => {
            // The JPEG encoder's meaningful range starts at 1; quality 0 maps to
            // its strongest compression setting while preserving the CLI's 0-100 scale.
            let pixels = image.to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality.max(1))
                .write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    image::ExtendedColorType::Rgb8,
                )?;
        }
        OutputFormat::WebP => {
            let pixels = if image.color().has_alpha() {
                DynamicImage::ImageRgba8(image.to_rgba8())
            } else {
                DynamicImage::ImageRgb8(image.to_rgb8())
            };
            bytes.extend_from_slice(
                &webp::Encoder::from_image(&pixels)
                    .map_err(|error| {
                        image::ImageError::Encoding(image::error::EncodingError::new(
                            image::error::ImageFormatHint::Name("WebP".into()),
                            error,
                        ))
                    })?
                    .encode(quality as f32),
            );
        }
        OutputFormat::Png => image.write_to(&mut Cursor::new(&mut bytes), format.to_format())?,
    }
    Ok(bytes)
}

#[cfg(all(test, feature = "native-heic"))]
pub(super) fn try_generate_heic_preview_with_native(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    format: OutputFormat,
    full: bool,
    quality: u8,
) -> Result<(), String> {
    use heic::{DecoderConfig, PixelLayout};

    if full {
        return Err("native HEIC full-size decoding is not selected by the hybrid backend".into());
    }

    let data = fs::read(input_path)
        .map_err(|e| format!("Failed to read HEIC file {}: {}", input_path.display(), e))?;

    // A thumbnail avoids decoding the complete HEVC frame. Only use it when it
    // is large enough to satisfy the requested preview size; otherwise the
    // ImageMagick fallback produces a higher-resolution result.
    let output = DecoderConfig::new()
        .decode_thumbnail(&data, PixelLayout::Rgb8)
        .map_err(|e| {
            format!(
                "Native HEIC thumbnail decode failed for {}: {}",
                input_path.display(),
                e
            )
        })?
        .ok_or_else(|| {
            format!(
                "HEIC file has no embedded thumbnail: {}",
                input_path.display()
            )
        })?;

    let thumbnail_max_dimension = output.width.max(output.height);
    if thumbnail_max_dimension < max_dimension {
        return Err(format!(
            "embedded HEIC thumbnail ({}px) is smaller than requested preview ({}px)",
            thumbnail_max_dimension, max_dimension
        ));
    }

    let image =
        image::RgbImage::from_raw(output.width, output.height, output.data).ok_or_else(|| {
            format!(
                "Native HEIC decoder returned invalid pixels for {}",
                input_path.display()
            )
        })?;
    let image = DynamicImage::ImageRgb8(image);

    let resized = super::resize::preview(image, max_dimension, full);

    let bytes = encode_image(&resized, format, quality).map_err(|e| {
        format!(
            "Failed to encode native HEIC preview {}: {}",
            input_path.display(),
            e
        )
    })?;
    fs::write(output_path, bytes).map_err(|e| {
        format!(
            "Failed to write native HEIC preview {}: {}",
            output_path.display(),
            e
        )
    })?;

    Ok(())
}

#[cfg(test)]
pub(super) fn try_generate_heic_preview_with_magick(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    full: bool,
    quality: u8,
) -> Result<bool, String> {
    let mut command = ProcessCommand::new("magick");
    command.arg(input_path).arg("-auto-orient").arg("-strip");

    if !full {
        command
            .arg("-resize")
            .arg(format!("{max_dimension}x{max_dimension}>"));
    }

    let output = match command
        .arg("-quality")
        .arg(quality.max(1).to_string())
        .arg(output_path)
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(format!(
                "Failed to launch ImageMagick for {}: {}",
                input_path.display(),
                err
            ));
        }
    };

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "ImageMagick failed to convert {}: {}",
        input_path.display(),
        stderr.trim()
    ))
}
