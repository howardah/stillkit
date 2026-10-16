use super::*;
use image::{DynamicImage, ImageEncoder, RgbImage};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn built_in_writer_adds_metadata_before_creating_output() {
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/reference.jpg");
    let output = std::env::temp_dir().join(format!(
        "stillkit-metadata-memory-write-{}.jpg",
        std::process::id()
    ));
    let _ = fs::remove_file(&output);
    let image = DynamicImage::ImageRgb8(RgbImage::new(7, 5));
    let mut encoded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut encoded)
        .write_image(image.as_bytes(), 7, 5, image.color().into())
        .unwrap();

    write_with_metadata(
        &input,
        &output,
        EncodedImage {
            bytes: encoded,
            width: 7,
            height: 5,
        },
        None,
        false,
        super::super::Pipeline::BuiltIn,
        &|_| panic!("built-in metadata must not invoke an external tool"),
    )
    .unwrap();

    assert_eq!(image::image_dimensions(&output).unwrap(), (7, 5));
    let file = fs::File::open(&output).unwrap();
    let metadata = exif::Reader::new()
        .read_from_container(&mut std::io::BufReader::new(file))
        .unwrap();
    assert_eq!(
        metadata
            .get_field(Tag::PixelXDimension, In::PRIMARY)
            .and_then(|field| field.value.get_uint(0)),
        Some(7)
    );
    fs::remove_file(output).unwrap();
}

#[test]
fn ordinary_and_heic_previews_use_native_metadata_before_exiftool() {
    for relative_input in ["test/reference.jpg", "test/DSCF1164.HEIC"] {
        let input = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_input);
        let output = std::env::temp_dir().join(format!(
            "stillkit-native-metadata-auto-{}-{}.jpg",
            std::process::id(),
            input.extension().unwrap().to_string_lossy()
        ));
        let _ = fs::remove_file(&output);
        let invoked_tool = AtomicBool::new(false);

        write_with_metadata(
            &input,
            &output,
            EncodedImage {
                bytes: valid_jpeg(7, 5),
                width: 7,
                height: 5,
            },
            None,
            false,
            super::super::Pipeline::Auto,
            &|_| invoked_tool.store(true, Ordering::Relaxed),
        )
        .unwrap();

        assert!(!invoked_tool.load(Ordering::Relaxed), "{relative_input}");
        assert_eq!(image::image_dimensions(&output).unwrap(), (7, 5));
        fs::remove_file(output).unwrap();
    }
}

#[test]
fn failed_metadata_copy_keeps_the_existing_preview() {
    let directory = std::env::temp_dir().join(format!(
        "stillkit-metadata-atomic-write-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("missing.jpg");
    let output = directory.join("preview.jpg");
    fs::write(&output, b"previous preview").unwrap();

    let error = write_with_metadata(
        &input,
        &output,
        EncodedImage {
            bytes: vec![0xff, 0xd8, 0xff, 0xd9],
            width: 1,
            height: 1,
        },
        None,
        false,
        super::super::Pipeline::BuiltIn,
        &|_| unreachable!("built-in pipeline must not invoke an external tool"),
    )
    .unwrap_err();

    assert!(error.contains("missing.jpg"));
    assert_eq!(fs::read(&output).unwrap(), b"previous preview");
    assert!(fs::read_dir(&directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".still-")
    }));
    fs::remove_dir_all(directory).unwrap();
}

fn valid_jpeg(width: u32, height: u32) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(RgbImage::new(width, height));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut bytes)
        .write_image(image.as_bytes(), width, height, image.color().into())
        .unwrap();
    bytes
}
