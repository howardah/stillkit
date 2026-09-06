use heic::heif::{Item, ItemType, Transform};
use image::{DynamicImage, metadata::Orientation};
use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};

// Optional acceleration: any uncertainty or decode failure returns None, allowing
// the caller to decode the primary image from the same input buffer.
pub(super) fn decode(data: &[u8], size: u32) -> Option<DynamicImage> {
    let container = heic::heif::parse(data, &heic::Unstoppable).ok()?;
    let primary = container.primary_item()?;
    let primary_dimensions = primary.dimensions?;
    let transforms = orientations(&primary)?;
    let info = heic::ImageInfo::from_bytes(data).ok()?;
    // An SDR JPEG is not a faithful replacement for a PQ/HLG primary image.
    if matches!(info.color_primaries, 9 | 12) || matches!(info.transfer_characteristics, 16 | 18) {
        return None;
    }
    let mut candidates: Vec<_> = container
        .find_thumbnails(primary.id)
        .into_iter()
        .filter_map(|id| container.get_item(id))
        .filter(|item| matches!(item.item_type, ItemType::Jpeg))
        .collect();
    candidates.sort_by_key(|item| {
        let (w, h) = item.dimensions.unwrap_or_default();
        (std::cmp::Reverse(u64::from(w) * u64::from(h)), item.id)
    });
    for candidate in candidates {
        if orientations(&candidate).as_ref() != Some(&transforms) {
            continue;
        }
        let Ok(jpeg) = container.get_item_data(candidate.id) else {
            continue;
        };
        let Some(mut image) = decode_jpeg(
            &jpeg,
            info.icc_profile.as_deref(),
            &candidate,
            primary_dimensions,
            size,
        ) else {
            continue;
        };
        for &orientation in &transforms {
            image.apply_orientation(orientation);
        }
        return Some(image);
    }
    None
}

fn decode_jpeg(
    data: &[u8],
    primary_icc: Option<&[u8]>,
    candidate: &Item,
    primary_dimensions: (u32, u32),
    size: u32,
) -> Option<DynamicImage> {
    // Some JPEG decoders tolerate truncation. Require a complete item as well as
    // successful pixel decoding, rather than accepting a partial camera preview.
    if !data.starts_with(&[0xff, 0xd8]) || !data.ends_with(&[0xff, 0xd9]) {
        return None;
    }
    let options = DecoderOptions::default()
        .set_strict_mode(true)
        .set_max_width(8192)
        .set_max_height(8192)
        .jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(data), options);
    decoder.decode_headers().ok()?;
    let (width, height) = decoder.dimensions()?;
    let dimensions = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    if candidate
        .dimensions
        .is_some_and(|expected| expected != dimensions)
        || !suitable_size(dimensions, primary_dimensions, size)
        || !unrotated_exif(decoder.exif().map(Vec::as_slice))
        || decoder.output_buffer_size()? > 128 * 1024 * 1024
    {
        return None;
    }
    // Output metadata comes from the primary image. Do not attach a different
    // primary ICC profile to the JPEG's pixels.
    if decoder.icc_profile().as_deref() != primary_icc {
        return None;
    }
    if !super::jpeg_validation::complete_baseline(data) {
        return None;
    }
    image::RgbImage::from_raw(dimensions.0, dimensions.1, decoder.decode().ok()?)
        .map(DynamicImage::ImageRgb8)
}

fn unrotated_exif(data: Option<&[u8]>) -> bool {
    let Some(data) = data else { return true };
    let data = data.strip_prefix(b"Exif\0\0").unwrap_or(data);
    let Ok(exif) = exif::Reader::new().read_raw(data.to_vec()) else {
        return false;
    };
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .is_none_or(|field| field.value.get_uint(0) == Some(1))
}

fn suitable_size((w, h): (u32, u32), (pw, ph): (u32, u32), size: u32) -> bool {
    if w == 0 || h == 0 || pw == 0 || ph == 0 || w > pw || h > ph {
        return false;
    }
    // Both axes must meet the requested output without upscaling. Permit one
    // percent aspect-ratio drift for rounded thumbnail dimensions, not crops.
    let scale = (f64::from(size) / f64::from(pw.max(ph))).min(1.0);
    let ratio = (f64::from(w) * f64::from(ph)) / (f64::from(h) * f64::from(pw));
    f64::from(w) >= (f64::from(pw) * scale).floor().max(1.0)
        && f64::from(h) >= (f64::from(ph) * scale).floor().max(1.0)
        && (ratio - 1.0).abs() <= 0.01
}

fn orientations(item: &Item) -> Option<Vec<Orientation>> {
    item.transforms
        .iter()
        .map(|transform| match transform {
            Transform::Rotation(rotation) => match rotation.angle {
                0 => Some(Orientation::NoTransforms),
                // The pinned HEIF parser converts irot's CCW bits to CW degrees.
                90 => Some(Orientation::Rotate90),
                180 => Some(Orientation::Rotate180),
                270 => Some(Orientation::Rotate270),
                _ => None,
            },
            Transform::Mirror(mirror) => match mirror.axis {
                0 => Some(Orientation::FlipHorizontal),
                1 => Some(Orientation::FlipVertical),
                _ => None,
            },
            // A crop requires mapping its coordinates to the thumbnail; use the
            // primary decoder instead of guessing how the camera rendered it.
            Transform::CleanAperture(_) => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(|items| {
            items
                .into_iter()
                .filter(|item| *item != Orientation::NoTransforms)
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_checks_both_axes_and_rejects_different_crops() {
        assert!(suitable_size((1920, 1280), (7728, 5152), 1000));
        assert!(!suitable_size((640, 480), (7728, 5152), 1000));
        assert!(!suitable_size((1920, 1080), (7728, 5152), 1000));
        assert!(!suitable_size((1920, 1280), (7728, 5152), 2000));
        assert!(!suitable_size((0, 1280), (7728, 5152), 1000));
        assert!(suitable_size((64, 64), (64, 64), 1000));
    }

    #[test]
    #[ignore = "requires local demo/DSCF0656.HEIC"]
    fn camera_sample_uses_large_embedded_jpeg() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/DSCF0656.HEIC");
        let data = std::fs::read(path).unwrap();
        let image = decode(&data, 1000).expect("camera's embedded JPEG should qualify");
        assert_eq!((image.width(), image.height()), (1920, 1280));
        assert!(
            decode(&data, 2000).is_none(),
            "must not upscale an embedded preview"
        );
    }

    #[test]
    fn invalid_or_ambiguous_jpeg_orientation_requires_primary_decoding() {
        assert!(unrotated_exif(None));
        assert!(!unrotated_exif(Some(b"corrupt")));
        for value in [1, 6, 8] {
            let field = exif::Field {
                tag: exif::Tag::Orientation,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Short(vec![value]),
            };
            let mut writer = exif::experimental::Writer::new();
            writer.push_field(&field);
            let mut data = std::io::Cursor::new(Vec::new());
            writer.write(&mut data, false).unwrap();
            assert_eq!(unrotated_exif(Some(data.get_ref())), value == 1);
        }
    }
}
