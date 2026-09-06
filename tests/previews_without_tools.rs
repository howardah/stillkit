use exif::{In, Tag, Value};
use image::{DynamicImage, GenericImageView};
use img_parts::{ImageEXIF, ImageICC};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

mod common;
use common::{TestDir, dng, field};

fn preview(input: &Path, output: &Path, format: &str, flags: &[&str]) -> PathBuf {
    let result = Command::new(env!("CARGO_BIN_EXE_still"))
        .env("PATH", "")
        .arg("previews")
        .arg(input)
        .arg("-o")
        .arg(output)
        .args(["-f", format, "-s", "16"])
        .args(flags)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&result.stderr).contains("Error"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let path = output
        .join(input.file_name().unwrap())
        .with_extension(format);
    assert!(
        path.exists(),
        "preview missing: stdout={}, stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    path
}

#[test]
fn multiple_images_default_to_preview_beside_the_first_input() {
    let dir = TestDir::new();
    let first_dir = dir.0.join("first");
    let second_dir = dir.0.join("second");
    fs::create_dir_all(&first_dir).unwrap();
    fs::create_dir_all(&second_dir).unwrap();
    let first = first_dir.join("one.jpg");
    let second = second_dir.join("two.jpg");
    DynamicImage::new_rgb8(8, 6).save(&first).unwrap();
    DynamicImage::new_rgb8(6, 8).save(&second).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_still"))
        .env("PATH", "")
        .args(["previews", "--no-deps", "--clear-metadata"])
        .arg(&first)
        .arg(&second)
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = first_dir.join("preview");
    assert!(output.join("one.jpg").is_file());
    assert!(output.join("two.jpg").is_file());
    assert!(!second_dir.join("preview").exists());
}

#[test]
fn max_size_full_keeps_original_dimensions() {
    let dir = TestDir::new();
    let input = dir.0.join("photo.jpg");
    DynamicImage::new_rgb8(32, 24).save(&input).unwrap();
    let output = dir.0.join("output");

    let result = Command::new(env!("CARGO_BIN_EXE_still"))
        .env("PATH", "")
        .args([
            "previews",
            "--no-deps",
            "--clear-metadata",
            "--max-size",
            "full",
        ])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        image::open(output.join("photo.jpg")).unwrap().dimensions(),
        (32, 24)
    );
}

#[test]
fn develops_raw_sensor_pixels_without_external_tools_or_embedded_jpeg() {
    let dir = TestDir::new();
    let input = dir.0.join("camera.DNG");
    dng(&input);
    let original = fs::read(&input).unwrap();
    for format in ["jpg", "png", "webp"] {
        let output = preview(&input, &dir.0.join(format), format, &["--full"]);
        let decoded = image::open(&output).unwrap();
        assert_eq!(decoded.dimensions(), (24, 32));
        let rgb = decoded.to_rgb8();
        assert!(rgb.as_raw().iter().max() > rgb.as_raw().iter().min());
        let metadata = exif::Reader::new()
            .read_from_container(&mut Cursor::new(fs::read(output).unwrap()))
            .unwrap();
        assert_eq!(
            metadata
                .get_field(Tag::Orientation, In::PRIMARY)
                .unwrap()
                .value
                .get_uint(0),
            Some(1)
        );
        assert!(
            metadata
                .get_field(Tag::DateTimeOriginal, In::PRIMARY)
                .is_some()
        );
    }
    let output = preview(&input, &dir.0.join("small"), "jpg", &["--clear-metadata"]);
    assert_eq!(image::open(&output).unwrap().dimensions(), (12, 16));
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn preserves_metadata_and_can_strip_it_without_exiftool() {
    let dir = TestDir::new();
    let input = dir.0.join("photo.jpg");
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(32, 24)
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .unwrap();
    let mut jpeg = img_parts::jpeg::Jpeg::from_bytes(bytes.into_inner().into()).unwrap();
    let artist = field(Tag::Artist, Value::Ascii(vec![b"Stillkit test".to_vec()]));
    let mut writer = exif::experimental::Writer::new();
    writer.push_field(&artist);
    let mut exif = Cursor::new(Vec::new());
    writer.write(&mut exif, false).unwrap();
    jpeg.set_exif(Some(exif.into_inner().into()));
    jpeg.set_icc_profile(Some(b"test profile".to_vec().into()));
    jpeg.segments_mut().insert(
        1,
        img_parts::jpeg::JpegSegment::new_with_contents(
            0xed,
            b"Photoshop 3.0\0\x38BIM\x04\x04\0\0\0\0\0\x09\x1c\x02\x78\0\x04test\0"
                .to_vec()
                .into(),
        ),
    );
    jpeg.segments_mut().insert(
        1,
        img_parts::jpeg::JpegSegment::new_with_contents(
            0xe1,
            b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>"
                .to_vec()
                .into(),
        ),
    );
    fs::write(&input, jpeg.encoder().bytes()).unwrap();
    for format in ["jpg", "png", "webp"] {
        for clear in [false, true] {
            let output = preview(
                &input,
                &dir.0.join(format!("{format}-{clear}")),
                format,
                if clear { &["--clear-metadata"] } else { &[] },
            );
            let data = fs::read(output).unwrap();
            let container = img_parts::DynImage::from_bytes(data.clone().into())
                .unwrap()
                .unwrap();
            assert_eq!(container.exif().is_none(), clear);
            assert_eq!(container.icc_profile().is_none(), clear);
            match &container {
                img_parts::DynImage::Jpeg(jpeg) => {
                    assert_eq!(jpeg.segments().iter().any(|s| s.marker() == 0xed), !clear)
                }
                img_parts::DynImage::Png(png) => assert_eq!(
                    png.chunks().iter().any(|chunk| chunk.kind() == *b"tEXt"
                        && chunk
                            .contents()
                            .windows(18)
                            .any(|value| value == b"1c0278000474657374")),
                    !clear
                ),
                _ => {}
            }
            use image::ImageDecoder;
            let mut decoder = image::ImageReader::new(Cursor::new(data))
                .with_guessed_format()
                .unwrap()
                .into_decoder()
                .unwrap();
            assert_eq!(decoder.xmp_metadata().unwrap().is_none(), clear);
            if !clear {
                let exif = exif::Reader::new()
                    .read_raw(container.exif().unwrap().to_vec())
                    .unwrap();
                assert!(exif.get_field(Tag::Artist, In::PRIMARY).is_some());
                assert_eq!(
                    exif.get_field(Tag::PixelXDimension, In::PRIMARY)
                        .unwrap()
                        .value
                        .get_uint(0),
                    Some(16)
                );
            }
        }
    }
}

#[test]
#[ignore = "full Rust decoding of the large HEIC fixture is slow; run explicitly"]
fn heic_without_external_tools() {
    let dir = TestDir::new();
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/DSCF1164.HEIC");
    let output = preview(&input, &dir.0, "jpg", &[]);
    let decoded = image::open(output).unwrap();
    assert_eq!(decoded.width().max(decoded.height()), 16);
}

#[test]
#[ignore = "requires local demo/DSCF0656.HEIC; run with --release --ignored"]
fn heic_no_deps_preserves_image_content() {
    let dir = TestDir::new();
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/DSCF0656.HEIC");
    let original = fs::read(&input).expect("place the original DSCF0656.HEIC in demo/");
    let info = heic::ImageInfo::from_bytes(&original).unwrap();
    assert_eq!((info.bit_depth, info.chroma_format), (10, 2));
    let output = preview(&input, &dir.0, "png", &["--no-deps", "--clear-metadata"]);
    let decoded = image::open(output).unwrap().to_rgb8();
    assert_eq!(decoded.dimensions(), (16, 10));

    // Regional RGB means from an independent ImageMagick/libheif decode.
    // Dimensions alone passed with heic 0.1.4 even though the pixels were
    // almost entirely white with colored bands. Check the whole scene, with
    // tolerance for different color conversion and downsampling implementations.
    let expected = [
        [44, 33, 25],
        [81, 62, 48],
        [131, 108, 88],
        [197, 177, 153],
        [146, 132, 120],
        [138, 116, 99],
        [152, 133, 113],
        [216, 200, 177],
        [61, 46, 37],
        [52, 36, 27],
        [83, 74, 71],
        [223, 212, 200],
        [48, 36, 29],
        [40, 26, 20],
        [71, 58, 53],
        [204, 186, 171],
    ];
    let regions = image::imageops::resize(&decoded, 4, 4, image::imageops::FilterType::Triangle);
    for (index, (actual, expected)) in regions.pixels().zip(expected).enumerate() {
        for channel in 0..3 {
            assert!(
                (i32::from(actual[channel]) - expected[channel]).abs() <= 30,
                "region {index}, channel {channel}: got {}, expected {}",
                actual[channel],
                expected[channel]
            );
        }
    }
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn heic_422_10bit_previews_preserve_colors_without_tools() {
    let dir = TestDir::new();
    let input =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gradient-422-10bit.heic");
    let original = fs::read(&input).unwrap();
    let info = heic::ImageInfo::from_bytes(&original).unwrap();
    assert_eq!((info.bit_depth, info.chroma_format), (10, 2));
    for full in [false, true] {
        let flags = if full {
            vec!["--no-deps", "--clear-metadata", "--full"]
        } else {
            vec!["--no-deps", "--clear-metadata"]
        };
        let output = preview(&input, &dir.0.join(full.to_string()), "png", &flags);
        let decoded = image::open(output).unwrap().to_rgb8();
        let size = if full { 64 } else { 16 };
        assert_eq!(decoded.dimensions(), (size, size));
        for (x, y, pixel) in decoded.enumerate_pixels() {
            let blue = 255.0 * f64::from(y) / f64::from(size - 1);
            let expected = [255.0 - blue, 0.0, blue];
            for channel in 0..3 {
                assert!(
                    (f64::from(pixel[channel]) - expected[channel]).abs() < 20.0,
                    "full={full}, ({x},{y}), got {pixel:?}, expected {expected:?}"
                );
            }
        }
    }
    assert_eq!(fs::read(input).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn failed_accelerators_fall_through_to_imagemagick_six() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestDir::new();
    let input = dir.0.join("camera.DNG");
    dng(&input);
    let pixels = dir.0.join("converted.png");
    DynamicImage::new_rgb8(10, 5).save(&pixels).unwrap();
    let log = dir.0.join("calls");
    for program in ["sips", "magick", "convert"] {
        let path = dir.0.join(program);
        fs::write(&path, b"#!/bin/sh\nprintf '%s\\n' \"${0##*/}\" >> \"$STILL_TEST_LOG\"\nif [ \"${0##*/}\" = convert ]; then /bin/cat \"$STILL_TEST_PIXELS\"; else exit 1; fi\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let output = dir.0.join("output");
    let result = Command::new(env!("CARGO_BIN_EXE_still"))
        .env("PATH", &dir.0)
        .env("STILL_TEST_LOG", &log)
        .env("STILL_TEST_PIXELS", pixels)
        .arg("previews")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--clear-metadata")
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        image::open(output.join("camera.jpg")).unwrap().dimensions(),
        (10, 5)
    );
    assert_eq!(
        fs::read_to_string(log).unwrap(),
        if cfg!(target_os = "macos") {
            "sips\nmagick\nconvert\n"
        } else {
            "magick\nconvert\n"
        }
    );
}
