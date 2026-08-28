use super::OutputFormat;
use crate::shared::image::{is_heic_family, load_image};
use image::{DynamicImage, GenericImageView, ImageEncoder};
use std::{fs, io::Cursor, path::Path, process::Command as ProcessCommand};

pub(super) fn do_generate_preview(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    format: OutputFormat,
    full: bool,
    clear_metadata: bool,
    quality: u8,
) -> Result<(), String> {
    let orientation_normalized = generate_preview_image(
        input_path,
        output_path,
        max_dimension,
        format,
        full,
        clear_metadata,
        quality,
    )?;

    if !clear_metadata {
        copy_metadata_with_exiftool(input_path, output_path, orientation_normalized)?;
    }

    Ok(())
}

fn generate_preview_image(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    format: OutputFormat,
    full: bool,
    clear_metadata: bool,
    quality: u8,
) -> Result<bool, String> {
    if is_heic_family(input_path) {
        // macOS's ImageIO-backed `sips` can use Apple's hardware-accelerated
        // HEIF/HEVC pipeline. It preserves the existing metadata flow here;
        // clear-metadata mode stays on ImageMagick so stripping remains exact.
        if !clear_metadata
            && try_generate_heic_preview_with_sips(
                input_path,
                output_path,
                max_dimension,
                format,
                full,
                quality,
            )?
        {
            return Ok(true);
        }

        // The native path is opt-in because the pure-Rust HEIC decoder has an
        // AGPL-or-commercial license. It also leaves ImageMagick available for
        // files or metadata variants the native decoder cannot handle.
        #[cfg(feature = "native-heic")]
        if try_generate_heic_preview_with_native(
            input_path,
            output_path,
            max_dimension,
            format,
            full,
            quality,
        )
        .is_ok()
        {
            return Ok(true);
        }

        // HEIC-family files benefit from ImageMagick's decoder and auto-orientation
        // when available. This is also the compatibility fallback for the native path.
        if try_generate_heic_preview_with_magick(
            input_path,
            output_path,
            max_dimension,
            full,
            quality,
        )? {
            return Ok(true);
        }
    }

    let img = load_image(input_path)?;

    let resized: DynamicImage = if full {
        img
    } else {
        let (width, height) = img.dimensions();
        let (new_width, new_height) = calculate_dimensions(width, height, max_dimension);

        if new_width < width || new_height < height {
            img.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3)
        } else {
            img
        }
    };

    let bytes = encode_image(&resized, format, quality)
        .map_err(|e| format!("Failed to encode image {}: {}", input_path.display(), e))?;

    fs::write(output_path, &bytes).map_err(|e| {
        format!(
            "Failed to write output file {}: {}",
            output_path.display(),
            e
        )
    })?;

    Ok(false)
}

pub(super) fn encode_image(
    image: &DynamicImage,
    format: OutputFormat,
    quality: u8,
) -> image::ImageResult<Vec<u8>> {
    let mut bytes = Vec::new();
    match format {
        OutputFormat::Jpeg => {
            // The JPEG encoder's meaningful range starts at 1; quality 0 maps to
            // its strongest compression setting while preserving the CLI's 0-100 scale.
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality.max(1))
                .write_image(
                    image.as_bytes(),
                    image.width(),
                    image.height(),
                    image.color().into(),
                )?;
        }
        OutputFormat::WebP => {
            bytes.extend_from_slice(
                &webp::Encoder::from_image(image)
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

#[cfg(target_os = "macos")]
fn try_generate_heic_preview_with_sips(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    format: OutputFormat,
    full: bool,
    quality: u8,
) -> Result<bool, String> {
    // sips supports JPEG, PNG, and several Apple image formats, but not WebP.
    // Restricting this fast path to JPEG keeps the output-format contract clear.
    if !matches!(format, OutputFormat::Jpeg) {
        return Ok(false);
    }

    let mut command = ProcessCommand::new("sips");
    command.arg("-s").arg("format").arg("jpeg");
    command
        .arg("-s")
        .arg("formatOptions")
        .arg(quality.max(1).to_string());

    if !full {
        command
            .arg("--resampleHeightWidthMax")
            .arg(max_dimension.to_string());
    }

    command.arg(input_path).arg("--out").arg(output_path);
    let output = match command.output() {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(format!(
                "Failed to launch sips for {}: {}",
                input_path.display(),
                err
            ));
        }
    };

    if output.status.success() {
        return Ok(true);
    }

    // A present sips binary may still reject a HEIF variant. Let ImageMagick
    // handle that case rather than making macOS less compatible than other OSes.
    Ok(false)
}

#[cfg(not(target_os = "macos"))]
fn try_generate_heic_preview_with_sips(
    _input_path: &Path,
    _output_path: &Path,
    _max_dimension: u32,
    _format: OutputFormat,
    _full: bool,
    _quality: u8,
) -> Result<bool, String> {
    Ok(false)
}

#[cfg(feature = "native-heic")]
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

    let (width, height) = image.dimensions();
    let (new_width, new_height) = calculate_dimensions(width, height, max_dimension);
    let resized = if new_width < width || new_height < height {
        image.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3)
    } else {
        image
    };

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

fn copy_metadata_with_exiftool(
    input_path: &Path,
    output_path: &Path,
    orientation_normalized: bool,
) -> Result<(), String> {
    let (width, height) = image::image_dimensions(output_path).map_err(|e| {
        format!(
            "Failed to read output dimensions for {}: {}",
            output_path.display(),
            e
        )
    })?;

    let mut command = ProcessCommand::new("exiftool");
    command
        .arg("-overwrite_original")
        .arg("-TagsFromFile")
        .arg(input_path)
        .arg("-EXIF:all")
        .arg("-XMP:all")
        .arg("-IPTC:all")
        .arg("-ICC_Profile")
        .arg(format!("-IFD0:ImageWidth={width}"))
        .arg(format!("-IFD0:ImageHeight={height}"))
        .arg(format!("-ExifImageWidth={width}"))
        .arg(format!("-ExifImageHeight={height}"));

    if orientation_normalized {
        command.arg("-Orientation#=1");
    }

    let output = match command.arg(output_path).output() {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "ExifTool is required to preserve metadata for {}. Install `exiftool` or rerun with --clear-metadata.",
                input_path.display()
            ));
        }
        Err(err) => {
            return Err(format!(
                "Failed to launch ExifTool for {}: {}",
                input_path.display(),
                err
            ));
        }
    };

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "ExifTool failed to copy metadata from {} to {}: {}",
        input_path.display(),
        output_path.display(),
        stderr.trim()
    ))
}

/// Calculate new dimensions maintaining aspect ratio
fn calculate_dimensions(width: u32, height: u32, max_dimension: u32) -> (u32, u32) {
    if width > height {
        let new_width = max_dimension;
        let new_height = (height as f32 * (new_width as f32 / width as f32)) as u32;
        (new_width, new_height.max(1))
    } else {
        let new_height = max_dimension;
        let new_width = (width as f32 * (new_height as f32 / height as f32)) as u32;
        (new_width.max(1), new_height)
    }
}
