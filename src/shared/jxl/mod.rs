use image::{DynamicImage, ImageBuffer};
use jxl_encoder::{LosslessConfig, LossyConfig, PixelLayout};
use jxl_oxide::{AuxBoxData, EnumColourEncoding, JxlImage, RenderingIntent};
use std::{fs, path::Path};

pub(crate) mod approval;
mod color;
mod container;
#[cfg(test)]
mod tests;

pub(crate) use container::{add_metadata, is_codestream};

#[derive(Clone, Copy, Default)]
pub(crate) struct Encoding {
    pub quality: u8,
    pub lossless: bool,
}

pub(crate) fn is_jxl(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jxl"))
}

pub(crate) fn parse(data: &[u8]) -> Result<JxlImage, String> {
    let image = JxlImage::builder().read(data).map_err(|e| e.to_string())?;
    let metadata = &image.image_header().metadata;
    if metadata.animation.is_some() {
        return Err("Animated JPEG XL is not supported; export a still image first".into());
    }
    let hdr_encoding = matches!(&metadata.colour_encoding,
        jxl_oxide::color::ColourEncoding::Enum(encoding)
            if matches!(encoding.tf, jxl_oxide::color::TransferFunction::Pq | jxl_oxide::color::TransferFunction::Hlg));
    let hdr_icc = image
        .original_icc()
        .and_then(|icc| moxcms::ColorProfile::new_from_slice(icc).ok())
        .and_then(|profile| profile.cicp)
        .is_some_and(|cicp| {
            matches!(
                cicp.transfer_characteristics,
                moxcms::TransferCharacteristics::Smpte2084 | moxcms::TransferCharacteristics::Hlg
            )
        });
    if hdr_encoding
        || hdr_icc
        || image.hdr_type().is_some()
        || metadata.bit_depth.bits_per_sample() > 16
        || matches!(
            metadata.bit_depth,
            jxl_oxide::image::BitDepth::FloatSample { .. }
        )
    {
        return Err(
            "HDR/float JPEG XL is not supported; export an 8-bit or 16-bit SDR image".into(),
        );
    }
    if !image.is_loading_done() {
        return Err("Incomplete JPEG XL image".into());
    }
    Ok(image)
}

pub(crate) fn load(path: &Path) -> Result<DynamicImage, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    decode(&data).map_err(|e| format!("Failed to decode {}: {e}", path.display()))
}

fn decode(data: &[u8]) -> Result<DynamicImage, String> {
    let mut image = parse(data)?;
    let sixteen = image.image_header().metadata.bit_depth.bits_per_sample() > 8;
    image.set_cms(jxl_oxide::Moxcms);
    let srgb_gray = matches!(&image.image_header().metadata.colour_encoding,
        jxl_oxide::color::ColourEncoding::Enum(encoding)
            if encoding.colour_space == jxl_oxide::color::ColourSpace::Grey
                && encoding.tf == jxl_oxide::color::TransferFunction::Srgb);
    // Expanding already-sRGB gray channels is exact; a CMS gray-to-RGB
    // transform can otherwise introduce rounding even in a lossless round trip.
    if !srgb_gray {
        image.request_color_encoding(EnumColourEncoding::srgb(RenderingIntent::Relative));
    }
    let rendered = image.render_frame(0).map_err(|e| e.to_string())?;
    let mut stream = rendered.stream();
    let (width, height, channels) = (stream.width(), stream.height(), stream.channels());
    let length = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(channels as usize))
        .ok_or("JXL dimensions overflow")?;
    macro_rules! pixels {
        ($sample:ty, $rgb:ident, $rgba:ident, $gray:ident, $graya:ident, $to_rgb:ident, $to_rgba:ident) => {{
            let mut pixels = Vec::<$sample>::new();
            pixels
                .try_reserve_exact(length)
                .map_err(|e| e.to_string())?;
            pixels.resize(length, 0);
            if stream.write_to_buffer(&mut pixels) != length {
                return Err("Incomplete JXL pixel buffer".into());
            }
            match channels {
                1 => ImageBuffer::from_raw(width, height, pixels)
                    .map(|p| DynamicImage::$rgb(DynamicImage::$gray(p).$to_rgb())),
                2 => ImageBuffer::from_raw(width, height, pixels)
                    .map(|p| DynamicImage::$rgba(DynamicImage::$graya(p).$to_rgba())),
                3 => ImageBuffer::from_raw(width, height, pixels).map(DynamicImage::$rgb),
                4 => ImageBuffer::from_raw(width, height, pixels).map(DynamicImage::$rgba),
                _ => None,
            }
            .ok_or_else(|| "Unsupported JXL channel layout".to_string())
        }};
    }
    if sixteen {
        pixels!(
            u16,
            ImageRgb16,
            ImageRgba16,
            ImageLuma16,
            ImageLumaA16,
            to_rgb16,
            to_rgba16
        )
    } else {
        pixels!(
            u8,
            ImageRgb8,
            ImageRgba8,
            ImageLuma8,
            ImageLumaA8,
            to_rgb8,
            to_rgba8
        )
    }
}

pub(crate) fn encode(image: &DynamicImage, options: Encoding) -> Result<Vec<u8>, String> {
    let layout = match image {
        DynamicImage::ImageRgb8(_) => PixelLayout::Rgb8,
        DynamicImage::ImageRgba8(_) => PixelLayout::Rgba8,
        DynamicImage::ImageLuma8(_) => PixelLayout::Gray8,
        DynamicImage::ImageLumaA8(_) => PixelLayout::GrayAlpha8,
        DynamicImage::ImageRgb16(_) => PixelLayout::Rgb16,
        DynamicImage::ImageRgba16(_) => PixelLayout::Rgba16,
        DynamicImage::ImageLuma16(_) => PixelLayout::Gray16,
        DynamicImage::ImageLumaA16(_) => PixelLayout::GrayAlpha16,
        _ => return Err("JXL output requires 8-bit or 16-bit SDR pixels".into()),
    };
    // Quality 100 is the highest lossy preset; only --lossless guarantees exact pixels.
    let quality = jxl_encoder::calibrated_jxl_quality(f32::from(options.quality)).min(99.0);
    let result = if options.lossless {
        LosslessConfig::new().with_effort(5).encode(
            image.as_bytes(),
            image.width(),
            image.height(),
            layout,
        )
    } else {
        LossyConfig::new(jxl_encoder::quality_to_distance(quality))
            .with_effort(5)
            .encode(image.as_bytes(), image.width(), image.height(), layout)
    };
    result.map_err(|e| format!("JXL encoding failed: {e}"))
}

pub(crate) fn exif(data: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let image = parse(data)?;
    Ok(
        match image.aux_boxes().first_exif().map_err(|e| e.to_string())? {
            AuxBoxData::Data(exif) => {
                Some(exif.payload()[exif.tiff_header_offset() as usize..].to_vec())
            }
            _ => None,
        },
    )
}

pub(crate) fn xmp(data: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let image = parse(data)?;
    Ok(match image.aux_boxes().first_xml() {
        AuxBoxData::Data(xml) => Some(xml.to_vec()),
        _ => None,
    })
}

pub(crate) fn srgb_profile() -> Result<Vec<u8>, String> {
    moxcms::ColorProfile::new_srgb()
        .encode()
        .map_err(|e| e.to_string())
}

pub(crate) fn normalize(image: DynamicImage, input: &Path) -> Result<DynamicImage, String> {
    if is_jxl(input) {
        return Ok(image);
    }
    color::normalize(image, input)
}
