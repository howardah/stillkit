#[cfg(target_os = "macos")]
use crate::shared::image::is_heic_family;
use crate::shared::{Pipeline, decoding, metadata};
use image::{DynamicImage, ImageEncoder, ImageFormat};
use std::{fs, path::Path, process::Command};

mod staged;

pub(super) fn apply_exposure(
    input: &Path,
    output: &Path,
    adjustment: f64,
    pipeline: Pipeline,
    replace: bool,
) -> Result<(), String> {
    let factor = exposure_factor(adjustment)?;
    let input = fs::canonicalize(input)
        .map_err(|e| format!("Failed to resolve {}: {e}", input.display()))?;
    let staging = staged::Output::new(output)?;
    let temporary = staging.path();
    convert(&input, temporary, factor, pipeline)?;
    metadata::copy_metadata(&input, temporary, true, pipeline)?;
    staging.persist(output, replace)
}

pub(super) fn exposure_factor(adjustment: f64) -> Result<f64, String> {
    let factor = 2f64.powf(adjustment);
    if !adjustment.is_finite() || !factor.is_finite() || factor <= 0.0 {
        return Err(format!(
            "Exposure adjustment is outside the supported range: {adjustment}"
        ));
    }
    Ok(factor)
}

fn convert(input: &Path, output: &Path, factor: f64, pipeline: Pipeline) -> Result<(), String> {
    let mut errors = Vec::new();
    if pipeline.allows_tools() {
        #[cfg(target_os = "macos")]
        if is_heic_family(input) || decoding::is_raw(input) {
            match decoding::with_sips(input, 0, true) {
                Ok(mut image) => {
                    multiply(&mut image, factor);
                    return encode(&image, output);
                }
                Err(error) => errors.push(error),
            }
        }
        for program in ["magick", "convert"] {
            match with_magick(program, input, output, factor) {
                Ok(()) => return Ok(()),
                Err(error) => errors.push(error),
            }
        }
    }
    let mut image = decoding::load_native(input).map_err(|e| {
        format!(
            "Failed to decode {}: {e}. {}",
            input.display(),
            errors.join("; ")
        )
    })?;
    multiply(&mut image, factor);
    encode(&image, output)
}

fn with_magick(program: &str, input: &Path, output: &Path, factor: f64) -> Result<(), String> {
    let mut first_frame = input.as_os_str().to_os_string();
    first_frame.push("[0]");
    let result = Command::new(program)
        .arg(first_frame)
        .args(["-auto-orient", "-channel", "RGB", "-evaluate", "multiply"])
        .arg(format!("{factor:.17}"))
        .args([
            "+channel",
            "-strip",
            "-quality",
            "90",
            "-define",
            "webp:lossless=true",
        ])
        .arg(output)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    image::image_dimensions(output)
        .map_err(|e| format!("{program} returned an invalid image: {e}"))?;
    Ok(())
}

fn encode(image: &DynamicImage, output: &Path) -> Result<(), String> {
    let format = ImageFormat::from_path(output).map_err(|e| e.to_string())?;
    if format == ImageFormat::Jpeg {
        let pixels = image.to_rgb8();
        let file = fs::File::create(output).map_err(|e| e.to_string())?;
        image::codecs::jpeg::JpegEncoder::new_with_quality(file, 90)
            .write_image(
                pixels.as_raw(),
                pixels.width(),
                pixels.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| e.to_string())
    } else {
        image
            .save_with_format(output, format)
            .map_err(|e| format!("Failed to encode {}: {e}", output.display()))
    }
}

fn scale<T: Copy>(samples: &mut [T], channels: usize, colors: usize, transform: impl Fn(T) -> T) {
    for pixel in samples.chunks_exact_mut(channels) {
        for value in &mut pixel[..colors] {
            *value = transform(*value);
        }
    }
}

fn multiply(image: &mut DynamicImage, factor: f64) {
    let channels = image.color().channel_count() as usize;
    let colors = channels - usize::from(image.color().has_alpha());
    let scale8 = |v: u8| (f64::from(v) * factor).round().clamp(0.0, 255.0) as u8;
    let scale16 = |v: u16| (f64::from(v) * factor).round().clamp(0.0, 65535.0) as u16;
    let scale32 = |v: f32| (f64::from(v) * factor).clamp(0.0, 1.0) as f32;
    match image {
        DynamicImage::ImageLuma8(p) => scale(p.as_mut(), channels, colors, scale8),
        DynamicImage::ImageLumaA8(p) => scale(p.as_mut(), channels, colors, scale8),
        DynamicImage::ImageRgb8(p) => scale(p.as_mut(), channels, colors, scale8),
        DynamicImage::ImageRgba8(p) => scale(p.as_mut(), channels, colors, scale8),
        DynamicImage::ImageLuma16(p) => scale(p.as_mut(), channels, colors, scale16),
        DynamicImage::ImageLumaA16(p) => scale(p.as_mut(), channels, colors, scale16),
        DynamicImage::ImageRgb16(p) => scale(p.as_mut(), channels, colors, scale16),
        DynamicImage::ImageRgba16(p) => scale(p.as_mut(), channels, colors, scale16),
        DynamicImage::ImageRgb32F(p) => scale(p.as_mut(), channels, colors, scale32),
        DynamicImage::ImageRgba32F(p) => scale(p.as_mut(), channels, colors, scale32),
        _ => unreachable!("image crate added a new pixel format"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_scale_colors_and_preserve_alpha_and_depth() {
        let mut rgba = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([30, 90, 200, 71]),
        ));
        multiply(&mut rgba, exposure_factor(1.0).unwrap());
        assert_eq!(rgba.to_rgba8().as_raw(), &[60, 180, 255, 71]);
        let mut gray =
            DynamicImage::ImageLuma16(image::ImageBuffer::from_pixel(1, 1, image::Luma([12345])));
        multiply(&mut gray, exposure_factor(-1.0).unwrap());
        assert_eq!(gray.as_luma16().unwrap().as_raw(), &[6173]);
        for stops in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 2000.0, -2000.0] {
            assert!(exposure_factor(stops).is_err());
        }
    }
}
