use exif::{In, Tag};
use image::{DynamicImage, GenericImageView};
use std::{fs, io::Cursor, path::Path, process::Command};
mod common;
use common::TestDir;

fn exposure(input: &Path, args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_still"))
        .env("PATH", "")
        .arg("exposure")
        .arg(input)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn missing_tools_adjusts_png_and_preserves_alpha() {
    let dir = TestDir::new();
    let input = dir.0.join("photo.PNG");
    DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        1,
        image::Rgba([30, 90, 200, 71]),
    ))
    .save(&input)
    .unwrap();
    let original = fs::read(&input).unwrap();
    let result = exposure(
        &input,
        &["-e".as_ref(), "1".as_ref(), "--next-to-original".as_ref()],
    );
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = dir.0.join("photo_+1_0.PNG");
    assert_eq!(
        image::open(&output).unwrap().to_rgba8().get_pixel(0, 0).0,
        [60, 180, 255, 71]
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    let generated = fs::read(&output).unwrap();
    let result = exposure(
        &input,
        &["-e".as_ref(), "1".as_ref(), "--next-to-original".as_ref()],
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("already exists"));
    assert_eq!(fs::read(&output).unwrap(), generated);
}

#[test]
fn raw_outputs_png_with_no_tools_and_overwrite_is_rejected() {
    let dir = TestDir::new();
    let input = dir.0.join("camera.DNG");
    common::dng(&input);
    let original = fs::read(&input).unwrap();
    let result = exposure(
        &input,
        &[
            "--no-deps".as_ref(),
            "-e".as_ref(),
            "0".as_ref(),
            "--next-to-original".as_ref(),
        ],
    );
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = dir.0.join("camera_+0_0.png");
    let image = image::open(&output).unwrap();
    assert_eq!(image.dimensions(), (24, 32));
    assert_eq!(image.color(), image::ColorType::Rgb16);
    let result = exposure(
        &input,
        &[
            "--no-deps".as_ref(),
            "-e".as_ref(),
            "0".as_ref(),
            "--overwrite".as_ref(),
        ],
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("Cannot overwrite HEIC/RAW"));
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn sixteen_bit_tiff_overwrite_preserves_pixels_and_metadata() {
    use exif::Value;
    let dir = TestDir::new();
    let input = dir.0.join("photo.tiff");
    let fields = [
        common::field(Tag::ImageWidth, Value::Long(vec![1])),
        common::field(Tag::ImageLength, Value::Long(vec![1])),
        common::field(Tag::BitsPerSample, Value::Short(vec![16, 16, 16])),
        common::field(Tag::Compression, Value::Short(vec![1])),
        common::field(Tag::PhotometricInterpretation, Value::Short(vec![2])),
        common::field(Tag::SamplesPerPixel, Value::Short(vec![3])),
        common::field(Tag::RowsPerStrip, Value::Long(vec![1])),
        common::field(Tag::Artist, Value::Ascii(vec![b"Test artist".to_vec()])),
    ];
    let pixels: Vec<u8> = [1000u16, 12000, 40000]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let strips = [pixels.as_slice()];
    let mut writer = exif::experimental::Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    writer.set_strips(&strips, In::PRIMARY);
    let mut data = Cursor::new(Vec::new());
    writer.write(&mut data, true).unwrap();
    fs::write(&input, data.into_inner()).unwrap();
    let result = exposure(
        &input,
        &[
            "--no-deps".as_ref(),
            "-e".as_ref(),
            "1".as_ref(),
            "--overwrite".as_ref(),
        ],
    );
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        image::open(&input)
            .unwrap()
            .as_rgb16()
            .unwrap()
            .get_pixel(0, 0)
            .0,
        [2000, 24000, 65535]
    );
    let exif = exif::Reader::new()
        .read_raw(fs::read(&input).unwrap())
        .unwrap();
    assert!(exif.get_field(Tag::Artist, In::PRIMARY).is_some());
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[test]
fn corrupt_input_is_not_damaged_by_overwrite_and_collisions_are_rejected() {
    let dir = TestDir::new();
    let input = dir.0.join("corrupt.png");
    fs::write(&input, b"not an image").unwrap();
    let result = exposure(
        &input,
        &[
            "--no-deps".as_ref(),
            "-e".as_ref(),
            "1".as_ref(),
            "--overwrite".as_ref(),
        ],
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("Failed to decode"));
    assert_eq!(fs::read(&input).unwrap(), b"not an image");
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    fs::write(dir.0.join("corrupt.CR2"), b"raw original").unwrap();
    let output = dir.0.join("output");
    let result = exposure(
        &dir.0,
        &[
            "--no-deps".as_ref(),
            "-e".as_ref(),
            "1".as_ref(),
            "-o".as_ref(),
            output.as_os_str(),
            "--original-names".as_ref(),
        ],
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("Multiple inputs produce"));
    assert!(!output.exists());
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        fs::rename(&input, dir.0.join("corrupt.PNG")).unwrap();
        let result = exposure(
            &dir.0,
            &[
                "--no-deps".as_ref(),
                "-e".as_ref(),
                "1".as_ref(),
                "-o".as_ref(),
                output.as_os_str(),
                "--original-names".as_ref(),
                "--force".as_ref(),
            ],
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("Multiple inputs produce"));
        assert!(!output.exists());
    }
}

#[test]
#[ignore = "full Rust HEIC decoding is slow; run explicitly"]
fn heic_exposure_uses_rust_without_tools() {
    let dir = TestDir::new();
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/DSCF1164.HEIC");
    let result = exposure(
        &input,
        &[
            "--no-deps".as_ref(),
            "-e".as_ref(),
            "-1".as_ref(),
            "-o".as_ref(),
            dir.0.as_os_str(),
        ],
    );
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let image = image::open(dir.0.join("DSCF1164_-1_0.png")).unwrap();
    assert!(image.width() > 1000 && image.height() > 1000);
}

#[test]
fn rust_and_imagemagick_agree_on_lossless_exposure() {
    if !["magick", "convert"].iter().any(|program| {
        Command::new(program)
            .arg("-version")
            .output()
            .is_ok_and(|result| result.status.success())
    }) {
        return;
    }
    let dir = TestDir::new();
    let input = dir.0.join("photo.png");
    DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        1,
        image::Rgba([31, 95, 210, 71]),
    ))
    .save(&input)
    .unwrap();
    for (folder, native) in [("auto", false), ("rust", true)] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_still"));
        command
            .arg("exposure")
            .arg(&input)
            .args(["-e", "0.5", "-o"])
            .arg(dir.0.join(folder));
        if native {
            command.arg("--no-deps");
        }
        let result = command.output().unwrap();
        assert!(
            result.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let automatic = image::open(dir.0.join("auto/photo_+0_5.png"))
        .unwrap()
        .to_rgba8();
    let native = image::open(dir.0.join("rust/photo_+0_5.png"))
        .unwrap()
        .to_rgba8();
    for (actual, expected) in native.as_raw().iter().zip(automatic.as_raw()) {
        assert!(
            actual.abs_diff(*expected) <= 1,
            "Rust={actual}, ImageMagick={expected}"
        );
    }
    assert_eq!(native.get_pixel(0, 0).0[3], 71);
}

#[cfg(unix)]
#[test]
fn directory_processing_preserves_non_utf8_names() {
    use std::os::unix::ffi::OsStringExt;
    let dir = TestDir::new();
    let input = dir
        .0
        .join(std::ffi::OsString::from_vec(b"photo-\xff.png".to_vec()));
    let file = match fs::File::create(&input) {
        Ok(file) => file,
        // The macOS test filesystem rejects this fixture (EILSEQ); its sandbox
        // can instead return EPERM. Exercise non-UTF-8 paths where supported.
        Err(error)
            if cfg!(target_os = "macos")
                && (error.raw_os_error() == Some(92)
                    || error.kind() == std::io::ErrorKind::PermissionDenied) =>
        {
            eprintln!("Skipping non-UTF-8 filename test: {error}");
            return;
        }
        Err(error) => panic!("Failed to create non-UTF-8 fixture: {error}"),
    };
    DynamicImage::new_rgb8(2, 1)
        .write_to(&mut std::io::BufWriter::new(file), image::ImageFormat::Png)
        .unwrap();
    let output = dir.0.join("output");
    let result = exposure(
        &dir.0,
        &[
            "--no-deps".as_ref(),
            "-e".as_ref(),
            "1".as_ref(),
            "-o".as_ref(),
            output.as_os_str(),
        ],
    );
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        output
            .join(std::ffi::OsString::from_vec(
                b"photo-\xff_+1_0.png".to_vec()
            ))
            .exists()
    );
    assert!(input.exists());
}
