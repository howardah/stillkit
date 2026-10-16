use image::{DynamicImage, ImageBuffer, Rgb, Rgba};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn directory() -> Directory {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "still-jxl-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    Directory(path)
}
struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn still(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_still"))
        .env("PATH", "")
        .args(args)
        .output()
        .unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn read(path: &Path) -> jxl_oxide::JxlImage {
    jxl_oxide::JxlImage::open_with_defaults(path).unwrap()
}
fn make_jpeg(path: &Path, iptc: bool) {
    use img_parts::{
        ImageEXIF,
        jpeg::{Jpeg, JpegSegment},
    };
    let mut bytes = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(ImageBuffer::from_pixel(4, 3, Rgb([40, 80, 120])))
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .unwrap();
    let mut jpeg = Jpeg::from_bytes(bytes.into_inner().into()).unwrap();
    let field = exif::Field {
        tag: exif::Tag::DateTimeOriginal,
        ifd_num: exif::In::PRIMARY,
        value: exif::Value::Ascii(vec![b"2021:03:04 05:06:07".to_vec()]),
    };
    let mut writer = exif::experimental::Writer::new();
    writer.push_field(&field);
    let mut exif = std::io::Cursor::new(Vec::new());
    writer.write(&mut exif, false).unwrap();
    jpeg.set_exif(Some(exif.into_inner().into()));
    jpeg.segments_mut().insert(
        1,
        JpegSegment::new_with_contents(
            0xe1,
            b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta xmlns:x='adobe:ns:meta/'/>"
                .to_vec()
                .into(),
        ),
    );
    if iptc {
        let mut resource = b"Photoshop 3.0\0\x38BIM\x04\x04\0\0".to_vec();
        let payload = b"\x1c\x02\x19\0\x04test";
        resource.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        resource.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            resource.push(0);
        }
        jpeg.segments_mut()
            .insert(1, JpegSegment::new_with_contents(0xed, resource.into()));
    }
    fs::write(path, jpeg.encoder().bytes()).unwrap();
}

#[test]
fn previews_and_exposure_round_trip_sixteen_bit_alpha_without_tools() {
    let dir = directory();
    let input = dir.0.join("photo.PNG");
    let source = ImageBuffer::from_pixel(7, 5, Rgba([12345u16, 23456, 34567, 45678]));
    DynamicImage::ImageRgba16(source.clone())
        .save(&input)
        .unwrap();
    success(&still(&[
        "previews".as_ref(),
        input.as_os_str(),
        "--format".as_ref(),
        "jxl".as_ref(),
        "--lossless".as_ref(),
        "--full".as_ref(),
        "--clear-metadata".as_ref(),
    ]));
    let preview = dir.0.join("preview/photo.jxl");
    let decoded = read(&preview);
    let render = decoded.render_frame(0).unwrap();
    let mut actual = vec![0u16; 7 * 5 * 4];
    render.stream().write_to_buffer(&mut actual);
    assert_eq!(actual, *source.as_raw());
    assert!(decoded.aux_boxes().first_exif().unwrap().is_not_found());
    success(&still(&[
        "exposure".as_ref(),
        preview.as_os_str(),
        "-e".as_ref(),
        "1".as_ref(),
        "--next-to-original".as_ref(),
        "--lossless".as_ref(),
    ]));
    let adjusted = read(&dir.0.join("preview/photo_+1_0.jxl"));
    let render = adjusted.render_frame(0).unwrap();
    let mut actual = vec![0u16; 7 * 5 * 4];
    render.stream().write_to_buffer(&mut actual);
    assert_eq!(&actual[..4], &[24690, 46912, 65535, 45678]);
    // A JXL source can also produce ordinary JPEG previews.
    success(&still(&[
        "previews".as_ref(),
        preview.as_os_str(),
        "--output".as_ref(),
        "jpeg".as_ref(),
    ]));
    assert!(dir.0.join("preview/jpeg/photo.jpg").is_file());
    let uppercase = dir.0.join("UPPER.JXL");
    fs::copy(&preview, &uppercase).unwrap();
    success(&still(&[
        "exposure".as_ref(),
        uppercase.as_os_str(),
        "-e".as_ref(),
        "0".as_ref(),
        "--overwrite".as_ref(),
        "--format".as_ref(),
        "jxl".as_ref(),
        "--lossless".as_ref(),
    ]));
    assert!(uppercase.exists());
    assert!(
        !fs::read_dir(&dir.0)
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name() == "UPPER.jxl")
    );
    // Original source was never changed.
    assert_eq!(image::open(&input).unwrap().to_rgba16(), source);
}

#[test]
fn jxl_output_preserves_exif_xmp_and_has_explicit_clear_mode() {
    let dir = directory();
    let input = dir.0.join("photo.jpg");
    make_jpeg(&input, false);
    success(&still(&[
        "exposure".as_ref(),
        input.as_os_str(),
        "-e".as_ref(),
        "0".as_ref(),
        "--format".as_ref(),
        "jxl".as_ref(),
        "--output".as_ref(),
        dir.0.join("out").as_os_str(),
    ]));
    let image = read(&dir.0.join("out/photo_+0_0.jxl"));
    let exif = image.aux_boxes().first_exif().unwrap().unwrap();
    let exif = exif::Reader::new()
        .read_raw(exif.payload().to_vec())
        .unwrap();
    assert!(
        exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
            .is_some()
    );
    assert!(image.aux_boxes().first_xml().has_data());
    let library = dir.0.join("library");
    let result = still(&[
        "organize".as_ref(),
        "--input".as_ref(),
        dir.0.join("out").as_os_str(),
        "--output".as_ref(),
        library.as_os_str(),
        "--dry-run".as_ref(),
    ]);
    success(&result);
    assert!(String::from_utf8_lossy(&result.stdout).contains("2021-03-04"));
    assert!(!library.exists());
    success(&still(&[
        "previews".as_ref(),
        input.as_os_str(),
        "--format".as_ref(),
        "jxl".as_ref(),
        "--clear-metadata".as_ref(),
    ]));
    let image = read(&dir.0.join("preview/photo.jxl"));
    assert!(image.aux_boxes().first_exif().unwrap().is_not_found());
    assert!(image.aux_boxes().first_xml().is_not_found());
}

#[test]
fn iptc_requires_permission_and_dry_runs_do_not_create_output() {
    let dir = directory();
    let input = dir.0.join("photo.jpg");
    make_jpeg(&input, true);
    let original = fs::read(&input).unwrap();
    let args = [
        "previews".as_ref(),
        input.as_os_str(),
        "--format".as_ref(),
        "jxl".as_ref(),
    ];
    let result = still(&args);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--allow-iptc-loss"));
    assert!(!dir.0.join("preview").exists());
    let result = still(&[args.as_slice(), &["--dry-run".as_ref()]].concat());
    success(&result);
    assert!(String::from_utf8_lossy(&result.stdout).contains("require approval"));
    assert!(!dir.0.join("preview").exists());
    success(&still(
        &[args.as_slice(), &["--allow-iptc-loss".as_ref()]].concat(),
    ));
    assert!(dir.0.join("preview/photo.jxl").is_file());
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn format_conflicts_collisions_and_dry_exposure_leave_sources_untouched() {
    let dir = directory();
    let input = dir.0.join("photo.png");
    DynamicImage::new_rgb8(3, 2).save(&input).unwrap();
    let original = fs::read(&input).unwrap();
    let args = [
        "exposure".as_ref(),
        input.as_os_str(),
        "-e".as_ref(),
        "0".as_ref(),
        "--format".as_ref(),
        "jxl".as_ref(),
    ];
    assert!(
        !still(&[args.as_slice(), &["--overwrite".as_ref()]].concat())
            .status
            .success()
    );
    let out = dir.0.join("out");
    success(&still(
        &[
            args.as_slice(),
            &["--output".as_ref(), out.as_os_str(), "--dry-run".as_ref()],
        ]
        .concat(),
    ));
    assert!(!out.exists());
    success(&still(
        &[
            args.as_slice(),
            &["--output".as_ref(), out.as_os_str(), "--lossless".as_ref()],
        ]
        .concat(),
    ));
    let generated = fs::read(out.join("photo_+0_0.jxl")).unwrap();
    assert!(
        !still(&[args.as_slice(), &["--output".as_ref(), out.as_os_str()]].concat())
            .status
            .success()
    );
    assert_eq!(fs::read(out.join("photo_+0_0.jxl")).unwrap(), generated);
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn jxl_is_interoperable_with_reference_tools_when_available() {
    if Command::new("djxl").arg("--version").output().is_err()
        || Command::new("cjxl").arg("--version").output().is_err()
    {
        eprintln!("Skipping reference-codec test: cjxl/djxl unavailable");
        return;
    }
    let dir = directory();
    let input = dir.0.join("source.png");
    let source = ImageBuffer::from_fn(19, 13, |x, y| {
        Rgba([(x * 13) as u8, 83, (y * 17) as u8, (x * y) as u8])
    });
    DynamicImage::ImageRgba8(source.clone())
        .save(&input)
        .unwrap();
    for (name, flags) in [
        ("lossless", vec!["--lossless"]),
        ("lossy", vec!["--quality", "100"]),
    ] {
        let mut args = vec![
            "previews".as_ref(),
            input.as_os_str(),
            "--format".as_ref(),
            "jxl".as_ref(),
            "--output".as_ref(),
            name.as_ref(),
        ];
        args.extend(flags.iter().map(std::ffi::OsStr::new));
        success(&still(&args));
        let output = dir.0.join(name).join("source.jxl");
        let png = dir.0.join(format!("{name}.png"));
        success(
            &Command::new("djxl")
                .arg(&output)
                .arg(&png)
                .output()
                .unwrap(),
        );
        let decoded = image::open(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), source.dimensions());
        if name == "lossless" {
            assert_eq!(decoded, source);
        }
    }
    let reference = dir.0.join("reference.JXL");
    success(
        &Command::new("cjxl")
            .arg(&input)
            .arg(&reference)
            .args(["-d", "0", "-e", "1"])
            .output()
            .unwrap(),
    );
    success(&still(&[
        "previews".as_ref(),
        reference.as_os_str(),
        "--format".as_ref(),
        "png".as_ref(),
        "--output".as_ref(),
        "decoded".as_ref(),
    ]));
    assert_eq!(
        image::open(dir.0.join("decoded/reference.png"))
            .unwrap()
            .to_rgba8(),
        source
    );
}

#[test]
fn batches_continue_after_iptc_skip_and_recursive_scans_exclude_outputs() {
    let dir = directory();
    let photos = dir.0.join("photos");
    fs::create_dir(&photos).unwrap();
    make_jpeg(&photos.join("skip.jpg"), true);
    make_jpeg(&photos.join("keep.jpg"), false);
    let args = [
        "previews".as_ref(),
        photos.as_os_str(),
        "--format".as_ref(),
        "jxl".as_ref(),
        "--recursive".as_ref(),
    ];
    let result = still(&args);
    assert!(!result.status.success());
    assert!(photos.join("preview/keep.jxl").exists());
    assert!(!photos.join("preview/skip.jxl").exists());
    let result = still(&[args.as_slice(), &["--allow-iptc-loss".as_ref()]].concat());
    success(&result);
    assert!(photos.join("preview/skip.jxl").exists());
    assert!(!photos.join("preview/preview").exists());
    // Changing formats cannot let two source files race to one destination.
    fs::copy(photos.join("preview/keep.jxl"), photos.join("keep.jxl")).unwrap();
    let result = still(&[
        "previews".as_ref(),
        photos.as_os_str(),
        "--format".as_ref(),
        "jxl".as_ref(),
        "--output".as_ref(),
        "collision".as_ref(),
    ]);
    assert!(!result.status.success());
    assert!(!photos.join("collision").exists());
}

#[test]
fn rgb_icc_is_normalized_before_jxl_encoding_and_alpha_is_preserved() {
    use img_parts::ImageICC;
    let dir = directory();
    let input = dir.0.join("p3.png");
    DynamicImage::ImageRgba16(ImageBuffer::from_pixel(
        3,
        2,
        Rgba([30000u16, 20000, 10000, 12345]),
    ))
    .save(&input)
    .unwrap();
    let mut png = img_parts::png::Png::from_bytes(fs::read(&input).unwrap().into()).unwrap();
    png.set_icc_profile(Some(
        moxcms::ColorProfile::new_display_p3()
            .encode()
            .unwrap()
            .into(),
    ));
    fs::write(&input, png.encoder().bytes()).unwrap();
    success(&still(&[
        "previews".as_ref(),
        input.as_os_str(),
        "--format".as_ref(),
        "jxl".as_ref(),
        "--lossless".as_ref(),
    ]));
    let decoded = read(&dir.0.join("preview/p3.jxl"));
    let render = decoded.render_frame(0).unwrap();
    let mut pixels = vec![0u16; 3 * 2 * 4];
    render.stream().write_to_buffer(&mut pixels);
    assert_eq!(pixels[3], 12345);
    assert_ne!(&pixels[..3], &[30000, 20000, 10000]);
    assert_eq!(
        decoded.image_header().metadata.bit_depth.bits_per_sample(),
        16
    );
}
