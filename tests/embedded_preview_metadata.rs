use image::{DynamicImage, Rgb, RgbImage};
use std::{fs, path::Path, process::Command};
mod common;
#[path = "common/heic.rs"]
mod heic_fixture;

fn preview(input: &Path, output: &Path, extra: &[&str]) -> Vec<u8> {
    let result = Command::new(env!("CARGO_BIN_EXE_still"))
        .env("PATH", "")
        .args([
            "previews",
            "--no-deps",
            "--max-size",
            "16",
            "--format",
            "png",
        ])
        .arg(input)
        .arg("--output")
        .arg(output)
        .args(extra)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::read(
        output
            .join(input.file_name().unwrap())
            .with_extension("png"),
    )
    .unwrap()
}

#[test]
fn embedded_pixels_follow_container_rotation_and_source_metadata_policy() {
    let dir = common::TestDir::new();
    let input = dir.0.join("rotated.HEIC");
    let image = DynamicImage::ImageRgb8(RgbImage::from_fn(32, 32, |_, y| {
        if y < 16 {
            Rgb([220, 0, 0])
        } else {
            Rgb([0, 220, 0])
        }
    }));
    let thumbnail = heic_fixture::Thumbnail {
        data: heic_fixture::jpeg(&image),
        rotation: Some(1),
        ..heic_fixture::Thumbnail::green(32)
    };
    let original = heic_fixture::with_thumbnails(&[thumbnail], Some(1));
    fs::write(&input, &original).unwrap();
    for clear in [false, true] {
        let data = preview(
            &input,
            &dir.0.join(clear.to_string()),
            if clear { &["--clear-metadata"] } else { &[] },
        );
        let pixels = image::load_from_memory(&data).unwrap().to_rgb8();
        // HEIF's quarter turn is counter-clockwise: top red becomes left red.
        assert!(
            pixels.get_pixel(0, 8)[0] > 180,
            "left={:?}, right={:?}",
            pixels.get_pixel(0, 8),
            pixels.get_pixel(15, 8)
        );
        assert!(
            pixels.get_pixel(15, 8)[1] > 180,
            "right={:?}",
            pixels.get_pixel(15, 8)
        );
        let metadata = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(data));
        if clear {
            assert!(metadata.is_err());
        } else {
            let metadata = metadata.unwrap();
            let artist = metadata
                .get_field(exif::Tag::Artist, exif::In::PRIMARY)
                .unwrap();
            assert!(
                matches!(&artist.value, exif::Value::Ascii(values) if values == &[b"Primary photo".to_vec()])
            );
            assert_eq!(
                metadata
                    .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                    .unwrap()
                    .value
                    .get_uint(0),
                Some(1)
            );
            assert_eq!(
                metadata
                    .get_field(exif::Tag::PixelXDimension, exif::In::PRIMARY)
                    .unwrap()
                    .value
                    .get_uint(0),
                Some(16)
            );
        }
    }
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn opt_out_also_bypasses_embedded_raw_jpegs() {
    let dir = common::TestDir::new();
    let input = dir.0.join("camera.DNG");
    common::dng(&input);
    let mut original = fs::read(&input).unwrap();
    original.extend(heic_fixture::Thumbnail::green(32).data);
    fs::write(&input, &original).unwrap();
    let embedded = preview(&input, &dir.0.join("embedded"), &["--clear-metadata"]);
    let primary = preview(
        &input,
        &dir.0.join("primary"),
        &["--clear-metadata", "--no-embedded-preview"],
    );
    let embedded = image::load_from_memory(&embedded).unwrap().to_rgb8();
    let primary = image::load_from_memory(&primary).unwrap().to_rgb8();
    assert_eq!(embedded.dimensions(), (16, 16));
    assert!(embedded.get_pixel(0, 0)[1] > 180);
    assert_eq!(primary.dimensions(), (12, 16));
    assert_eq!(fs::read(input).unwrap(), original);
}
