use super::batch::{parse_existing_file_action_input, preview_size_label};
#[cfg(feature = "native-heic")]
use super::image::try_generate_heic_preview_with_native;
use super::image::{do_generate_preview, encode_image, try_generate_heic_preview_with_magick};
use super::{ExistingFileAction, OutputFormat, subcommand};
use crate::shared::image::{is_heic_family, is_supported_image};
use image::GenericImageView;
use std::{fs, path::Path, process, process::Command as ProcessCommand};

fn magick_available() -> bool {
    ProcessCommand::new("magick")
        .arg("-version")
        .output()
        .is_ok()
}

fn exiftool_available() -> bool {
    ProcessCommand::new("exiftool").arg("-ver").output().is_ok()
}

fn identify_verbose(path: &Path) -> String {
    let output = ProcessCommand::new("identify")
        .arg("-verbose")
        .arg(path)
        .output()
        .expect("identify should run");

    assert!(
        output.status.success(),
        "identify should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("identify output should be valid utf-8")
}

fn exiftool_dump(path: &Path) -> String {
    let output = ProcessCommand::new("exiftool")
        .arg("-a")
        .arg("-G1")
        .arg("-s")
        .arg(path)
        .output()
        .expect("exiftool should run");

    assert!(
        output.status.success(),
        "exiftool should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("exiftool output should be valid utf-8")
}

#[test]
fn parses_existing_file_action_input() {
    assert_eq!(
        parse_existing_file_action_input("o"),
        Some(ExistingFileAction::Overwrite)
    );
    assert_eq!(
        parse_existing_file_action_input("overwrite"),
        Some(ExistingFileAction::Overwrite)
    );
    assert_eq!(
        parse_existing_file_action_input("s"),
        Some(ExistingFileAction::Skip)
    );
    assert_eq!(
        parse_existing_file_action_input(""),
        Some(ExistingFileAction::Skip)
    );
    assert_eq!(parse_existing_file_action_input("later"), None);
}

#[test]
fn quality_argument_accepts_only_zero_through_one_hundred() {
    let matches = subcommand()
        .try_get_matches_from(["previews", "--quality", "0"])
        .expect("zero should be a valid quality");
    assert_eq!(matches.get_one::<u8>("quality"), Some(&0));

    assert!(
        subcommand()
            .try_get_matches_from(["previews", "--quality", "101"])
            .is_err()
    );
}

#[test]
fn quality_controls_jpeg_and_webp_compression() {
    let image = image::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("test/reference.jpg"))
        .expect("reference image should be readable");

    for format in [OutputFormat::Jpeg, OutputFormat::WebP] {
        let compressed = encode_image(&image, format, 0).expect("encoding should succeed");
        let high_quality = encode_image(&image, format, 100).expect("encoding should succeed");
        assert!(
            compressed.len() < high_quality.len(),
            "quality should affect {} output size",
            format.extension()
        );
    }
}

#[test]
fn supports_hif_as_heic_family_image() {
    assert!(is_supported_image(Path::new("photo.hif")));
    assert!(is_supported_image(Path::new("photo.HIF")));
    assert!(is_heic_family(Path::new("photo.hif")));
}

#[test]
fn previews_heic_without_whiteout_when_magick_is_available() {
    if !magick_available() {
        return;
    }

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/DSCF1164.HEIC");
    let output_path =
        std::env::temp_dir().join(format!("stillkit-preview-heic-{}.jpg", process::id()));
    let _ = fs::remove_file(&output_path);

    try_generate_heic_preview_with_magick(&path, &output_path, 1000, false, 75)
        .expect("ImageMagick HEIC preview should succeed");

    let image = image::open(&output_path).expect("generated preview should be readable");
    let rgb = image.to_rgb8();

    let step_x = (rgb.width() / 16).max(1) as usize;
    let step_y = (rgb.height() / 16).max(1) as usize;
    let mut total = 0u64;
    let mut samples = 0u64;

    for y in (0..rgb.height() as usize).step_by(step_y) {
        for x in (0..rgb.width() as usize).step_by(step_x) {
            let pixel = rgb.get_pixel(x as u32, y as u32);
            total += u64::from(pixel[0]) + u64::from(pixel[1]) + u64::from(pixel[2]);
            samples += 3;
        }
    }

    let average = total as f64 / samples as f64;
    assert!(
        average < 220.0,
        "decoded HEIC output is unexpectedly blown out: {average}"
    );

    let _ = fs::remove_file(output_path);
}

#[cfg(feature = "native-heic")]
#[test]
#[ignore = "decoding the large 10-bit fixture is too slow for the normal test suite"]
fn native_heic_preview_is_readable() {
    let input_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/DSCF1164.HEIC");
    let output_path =
        std::env::temp_dir().join(format!("stillkit-native-heic-{}.jpg", process::id()));
    let _ = fs::remove_file(&output_path);

    try_generate_heic_preview_with_native(
        &input_path,
        &output_path,
        1000,
        OutputFormat::Jpeg,
        false,
        75,
    )
    .expect("native HEIC preview should succeed");

    let image = image::open(&output_path).expect("native preview should be readable");
    assert!(image.width() <= 1000);
    assert!(image.height() <= 1000);
    let _ = fs::remove_file(output_path);
}

#[test]
fn preview_size_label_reflects_full_mode() {
    assert_eq!(preview_size_label(1000, false), "max dimension: 1000");
    assert_eq!(preview_size_label(1000, true), "full size");
}

#[test]
fn full_mode_keeps_original_dimensions() {
    let input_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/reference.jpg");
    let source = image::open(&input_path).expect("reference image should be readable");
    let output_path =
        std::env::temp_dir().join(format!("stillkit-preview-full-{}.png", process::id()));
    let _ = fs::remove_file(&output_path);

    do_generate_preview(
        &input_path,
        &output_path,
        10,
        OutputFormat::Png,
        true,
        true,
        75,
    )
    .expect("full preview generation should succeed");

    let output = image::open(&output_path).expect("generated image should be readable");
    assert_eq!(output.dimensions(), source.dimensions());

    let _ = fs::remove_file(output_path);
}

#[test]
fn preserves_metadata_by_default_when_magick_is_available() {
    if !exiftool_available() {
        return;
    }

    let input_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/reference.jpg");
    let output_path =
        std::env::temp_dir().join(format!("stillkit-preview-meta-{}.jpg", process::id()));
    let _ = fs::remove_file(&output_path);

    do_generate_preview(
        &input_path,
        &output_path,
        10,
        OutputFormat::Jpeg,
        false,
        false,
        75,
    )
    .expect("metadata-preserving preview generation should succeed");

    let report = identify_verbose(&output_path);
    assert!(report.contains("Profile-exif:"), "missing EXIF profile");
    assert!(report.contains("Profile-xmp:"), "missing XMP profile");
    assert!(
        report.contains("exif:Artist:"),
        "missing EXIF artist metadata"
    );
    assert!(
        report.contains("xmp:Rating:"),
        "missing XMP rating metadata"
    );
    assert!(report.contains("Filesize:"), "missing filesize metadata");

    let (width, height) = image::image_dimensions(&output_path)
        .expect("generated preview dimensions should be readable");
    let metadata = exiftool_dump(&output_path);
    assert!(
        metadata.contains(&format!(
            "[File]          ImageWidth                      : {width}"
        )),
        "file image width should match preview output"
    );
    assert!(
        metadata.contains(&format!(
            "[File]          ImageHeight                     : {height}"
        )),
        "file image height should match preview output"
    );
    assert!(
        metadata.contains(&format!(
            "[ExifIFD]       ExifImageWidth                  : {width}"
        )),
        "embedded EXIF width should match preview output"
    );
    assert!(
        metadata.contains(&format!(
            "[ExifIFD]       ExifImageHeight                 : {height}"
        )),
        "embedded EXIF height should match preview output"
    );

    let _ = fs::remove_file(output_path);
}

#[test]
fn clear_metadata_strips_profiles_when_magick_is_available() {
    if !exiftool_available() {
        return;
    }

    let input_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/reference.jpg");
    let output_path =
        std::env::temp_dir().join(format!("stillkit-preview-clear-meta-{}.jpg", process::id()));
    let _ = fs::remove_file(&output_path);

    do_generate_preview(
        &input_path,
        &output_path,
        10,
        OutputFormat::Jpeg,
        false,
        true,
        75,
    )
    .expect("metadata-cleared preview generation should succeed");

    let report = identify_verbose(&output_path);
    assert!(!report.contains("Profile-exif:"), "unexpected EXIF profile");
    assert!(!report.contains("Profile-xmp:"), "unexpected XMP profile");
    assert!(
        !report.contains("exif:Artist:"),
        "unexpected EXIF artist metadata"
    );
    assert!(
        !report.contains("xmp:Rating:"),
        "unexpected XMP rating metadata"
    );
    assert!(report.contains("Filesize:"), "missing filesize metadata");

    let _ = fs::remove_file(output_path);
}

#[test]
fn heic_metadata_dimensions_match_preview_output() {
    if !magick_available() || !exiftool_available() {
        return;
    }

    let input_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/DSCF1164.HEIC");
    let output_path =
        std::env::temp_dir().join(format!("stillkit-preview-heic-meta-{}.jpg", process::id()));
    let _ = fs::remove_file(&output_path);

    do_generate_preview(
        &input_path,
        &output_path,
        1000,
        OutputFormat::Jpeg,
        false,
        false,
        75,
    )
    .expect("HEIC preview generation should succeed");

    let (width, height) = image::image_dimensions(&output_path)
        .expect("generated preview dimensions should be readable");
    let metadata = exiftool_dump(&output_path);
    assert!(
        metadata.contains(&format!(
            "[File]          ImageWidth                      : {width}"
        )),
        "file image width should match preview output"
    );
    assert!(
        metadata.contains(&format!(
            "[File]          ImageHeight                     : {height}"
        )),
        "file image height should match preview output"
    );
    assert!(
        metadata.contains(&format!(
            "[ExifIFD]       ExifImageWidth                  : {width}"
        )),
        "embedded EXIF width should match preview output"
    );
    assert!(
        metadata.contains(&format!(
            "[ExifIFD]       ExifImageHeight                 : {height}"
        )),
        "embedded EXIF height should match preview output"
    );
    assert!(
        metadata.contains("[XMP-xmp]       Rating                          : 0"),
        "XMP rating should be preserved"
    );

    let _ = fs::remove_file(output_path);
}
