use image::{DynamicImage, ImageDecoder};
use moxcms::{ColorProfile, Layout};
use std::path::Path;

pub(super) fn normalize(image: DynamicImage, input: &Path) -> Result<DynamicImage, String> {
    // HEIC and RAW development already supply RGB; their native profiles are
    // handled by their decoders. Ordinary raster profiles need an explicit transform.
    if crate::shared::image::is_heic_family(input) || crate::shared::decoding::is_raw(input) {
        return Ok(image);
    }
    let mut decoder = image::ImageReader::open(input)
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let Some(icc) = decoder.icc_profile().map_err(|e| e.to_string())? else {
        return Ok(image);
    };
    let source = ColorProfile::new_from_slice(&icc)
        .map_err(|e| format!("Invalid color profile in {}: {e}", input.display()))?;
    let target = ColorProfile::new_srgb();
    let alpha = image.color().has_alpha();
    let sixteen = image.color().bits_per_pixel() / u16::from(image.color().channel_count()) > 8;
    let layout = if alpha { Layout::Rgba } else { Layout::Rgb };
    let gray = source.color_space == moxcms::DataColorSpace::Gray;
    let source_layout = if gray {
        if alpha {
            Layout::GrayAlpha
        } else {
            Layout::Gray
        }
    } else {
        layout
    };
    macro_rules! transform {
        ($pixels:expr, $method:ident, $variant:ident) => {{
            let pixels = $pixels;
            let mut output = image::ImageBuffer::new(image.width(), image.height());
            source
                .$method(source_layout, &target, layout, Default::default())
                .and_then(|t| t.transform(pixels.as_raw(), output.as_mut()))
                .map_err(|e| {
                    format!("Cannot convert color profile for {}: {e}", input.display())
                })?;
            Ok(DynamicImage::$variant(output))
        }};
    }
    match (sixteen, alpha, gray) {
        (false, false, false) => transform!(image.to_rgb8(), create_transform_8bit, ImageRgb8),
        (false, true, false) => transform!(image.to_rgba8(), create_transform_8bit, ImageRgba8),
        (true, false, false) => transform!(image.to_rgb16(), create_transform_16bit, ImageRgb16),
        (true, true, false) => transform!(image.to_rgba16(), create_transform_16bit, ImageRgba16),
        (false, false, true) => transform!(image.to_luma8(), create_transform_8bit, ImageRgb8),
        (false, true, true) => {
            transform!(image.to_luma_alpha8(), create_transform_8bit, ImageRgba8)
        }
        (true, false, true) => transform!(image.to_luma16(), create_transform_16bit, ImageRgb16),
        (true, true, true) => {
            transform!(image.to_luma_alpha16(), create_transform_16bit, ImageRgba16)
        }
    }
}
