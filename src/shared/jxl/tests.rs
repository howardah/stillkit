use super::*;
use image::{Rgb, RgbImage, Rgba, RgbaImage};

#[test]
fn lossless_preserves_rgb_rgba_and_sixteen_bit_pixels() {
    let images = [
        DynamicImage::ImageRgb8(RgbImage::from_fn(19, 13, |x, y| {
            Rgb([(x * 13) as u8, (y * 17) as u8, 89])
        })),
        DynamicImage::ImageRgba8(RgbaImage::from_fn(19, 13, |x, y| {
            Rgba([(x * 13) as u8, 71, (y * 17) as u8, (x * y) as u8])
        })),
        DynamicImage::ImageRgb16(ImageBuffer::from_fn(19, 13, |x, y| {
            Rgb([(x * 3451) as u16, (y * 5003) as u16, 12345])
        })),
        DynamicImage::ImageRgba16(ImageBuffer::from_fn(19, 13, |x, y| {
            Rgba([
                (x * 3451) as u16,
                12345,
                (y * 5003) as u16,
                (x * y * 137) as u16,
            ])
        })),
    ];
    for image in images {
        let bytes = encode(
            &image,
            Encoding {
                quality: 75,
                lossless: true,
            },
        )
        .unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.color(), image.color());
        assert_eq!(decoded.as_bytes(), image.as_bytes());
    }
}

#[test]
fn lossy_quality_endpoints_produce_decodable_stills() {
    let image = DynamicImage::ImageRgb8(RgbImage::from_fn(17, 9, |x, y| {
        Rgb([(x * 15) as u8, (y * 29) as u8, 127])
    }));
    for quality in [0, 75, 100] {
        let encoded = encode(
            &image,
            Encoding {
                quality,
                lossless: false,
            },
        )
        .unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (17, 9));
    }
}

#[test]
fn metadata_container_round_trips_exif_xmp_and_rejects_truncation() {
    let bytes = encode(
        &DynamicImage::new_rgb8(3, 2),
        Encoding {
            quality: 75,
            lossless: true,
        },
    )
    .unwrap();
    let field = exif::Field {
        tag: exif::Tag::DateTimeOriginal,
        ifd_num: exif::In::PRIMARY,
        value: exif::Value::Ascii(vec![b"2020:06:07 08:09:10".to_vec()]),
    };
    let mut writer = exif::experimental::Writer::new();
    writer.push_field(&field);
    let mut tiff = std::io::Cursor::new(Vec::new());
    writer.write(&mut tiff, false).unwrap();
    let xmp = b"<x:xmpmeta xmlns:x='adobe:ns:meta/'/>";
    let container = add_metadata(bytes, tiff.get_ref(), Some(xmp)).unwrap();
    assert_eq!(exif(&container).unwrap().unwrap(), *tiff.get_ref());
    assert_eq!(super::xmp(&container).unwrap().unwrap(), xmp);
    assert_eq!(decode(&container).unwrap().width(), 3);
    assert!(decode(&container[..container.len() - 10]).is_err());
    assert!(decode(b"not a jxl").is_err());
}

#[test]
fn input_detection_is_case_insensitive_and_path_safe() {
    for name in ["photo.jxl", "photo.JXL", "photo.JxL"] {
        assert!(is_jxl(Path::new(name)));
        assert!(crate::shared::image::is_supported_image(Path::new(name)));
    }
    assert!(!is_jxl(Path::new("photo")));
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"\xff.JXL".to_vec()));
        assert!(is_jxl(&path));
    }
}

#[test]
fn animated_hdr_and_float_output_are_rejected_explicitly() {
    let pixels = vec![10u8; 4 * 3 * 3];
    let frames = [
        jxl_encoder::AnimationFrame {
            pixels: &pixels,
            duration: 10,
        },
        jxl_encoder::AnimationFrame {
            pixels: &pixels,
            duration: 10,
        },
    ];
    let bytes = LosslessConfig::new()
        .with_effort(1)
        .encode_animation(4, 3, PixelLayout::Rgb8, &Default::default(), &frames)
        .unwrap();
    assert!(parse(&bytes).err().unwrap().contains("Animated"));
    let icc = moxcms::ColorProfile::new_display_p3_pq().encode().unwrap();
    let metadata = jxl_encoder::ImageMetadata::new().with_icc_profile(&icc);
    let bytes = LosslessConfig::new()
        .with_effort(1)
        .encode_request(4, 3, PixelLayout::Rgb8)
        .with_metadata(&metadata)
        .encode(&pixels)
        .unwrap();
    assert!(parse(&bytes).err().unwrap().contains("HDR/float"));
    let float = DynamicImage::ImageRgb32F(ImageBuffer::from_pixel(4, 3, Rgb([0.5f32; 3])));
    assert!(
        encode(
            &float,
            Encoding {
                quality: 75,
                lossless: true
            }
        )
        .unwrap_err()
        .contains("SDR")
    );
}

#[test]
fn grayscale_pixels_render_to_srgb_without_losing_precision() {
    for source in [
        DynamicImage::ImageLuma8(ImageBuffer::from_pixel(3, 2, image::Luma([87u8]))),
        DynamicImage::ImageLuma16(ImageBuffer::from_pixel(3, 2, image::Luma([12345u16]))),
    ] {
        let bytes = encode(
            &source,
            Encoding {
                quality: 75,
                lossless: true,
            },
        )
        .unwrap();
        let actual = decode(&bytes).unwrap();
        assert_eq!(actual.to_rgb16(), source.to_rgb16());
    }
}
