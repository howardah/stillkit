use exif::{Field, In, Tag, Value};
use image::{DynamicImage, GenericImageView};
use img_parts::{ImageEXIF, ImageICC};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "still-no-tools-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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

fn field(tag: Tag, value: Value) -> Field {
    Field {
        tag,
        value,
        ifd_num: In::PRIMARY,
    }
}

fn dng(path: &Path) {
    let tag = |id| Tag(exif::Context::Tiff, id);
    let fields = vec![
        field(Tag::ImageWidth, Value::Long(vec![32])),
        field(Tag::ImageLength, Value::Long(vec![24])),
        field(Tag::BitsPerSample, Value::Short(vec![16])),
        field(Tag::Compression, Value::Short(vec![1])),
        field(Tag::PhotometricInterpretation, Value::Short(vec![32803])),
        field(Tag::SamplesPerPixel, Value::Short(vec![1])),
        field(Tag::RowsPerStrip, Value::Long(vec![24])),
        field(Tag::Make, Value::Ascii(vec![b"Stillkit".to_vec()])),
        field(Tag::Model, Value::Ascii(vec![b"Synthetic Bayer".to_vec()])),
        field(Tag::Orientation, Value::Short(vec![6])),
        field(
            Tag::DateTimeOriginal,
            Value::Ascii(vec![b"2026:09:05 12:00:00".to_vec()]),
        ),
        field(tag(33421), Value::Short(vec![2, 2])),
        field(tag(33422), Value::Byte(vec![0, 1, 1, 2])),
        field(tag(50706), Value::Byte(vec![1, 4, 0, 0])),
        field(tag(50707), Value::Byte(vec![1, 1, 0, 0])),
        field(
            tag(50708),
            Value::Ascii(vec![b"Stillkit Synthetic Bayer".to_vec()]),
        ),
        field(tag(50714), Value::Long(vec![0])),
        field(tag(50717), Value::Long(vec![65535])),
        field(
            tag(50721),
            Value::SRational(
                (0..9)
                    .map(|i| exif::SRational {
                        num: i32::from(i % 4 == 0),
                        denom: 1,
                    })
                    .collect(),
            ),
        ),
        field(
            tag(50728),
            Value::Rational(vec![exif::Rational { num: 1, denom: 1 }; 3]),
        ),
        field(tag(50778), Value::Short(vec![21])),
    ];
    let pixels: Vec<u8> = (0..32 * 24)
        .flat_map(|i| (1000 + i as u16 * 60).to_le_bytes())
        .collect();
    let strips = [pixels.as_slice()];
    let mut writer = exif::experimental::Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    writer.set_strips(&strips, In::PRIMARY);
    let mut data = Cursor::new(Vec::new());
    writer.write(&mut data, true).unwrap();
    fs::write(path, data.into_inner()).unwrap();
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
