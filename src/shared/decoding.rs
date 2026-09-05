use image::{DynamicImage, ImageDecoder};
use std::{fs, io::Cursor, path::Path, process::Command};

pub(crate) fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "cr2"
                    | "cr3"
                    | "crw"
                    | "nef"
                    | "nrw"
                    | "arw"
                    | "sr2"
                    | "srf"
                    | "raf"
                    | "dng"
                    | "orf"
                    | "rw2"
                    | "pef"
                    | "srw"
                    | "raw"
                    | "rwl"
                    | "3fr"
                    | "fff"
                    | "iiq"
                    | "mos"
                    | "mrw"
                    | "erf"
                    | "kdc"
                    | "dcr"
            )
        })
}

// Decode tool output back to pixels so all formats share the same encoder and
// metadata policy, including sips conversions requested with --clear-metadata.
pub(crate) fn load_preview(
    path: &Path,
    size: u32,
    full: bool,
    pipeline: super::Pipeline,
) -> Result<DynamicImage, String> {
    let path = fs::canonicalize(path)
        .map_err(|e| format!("Failed to resolve RAW input {}: {e}", path.display()))?;
    let mut errors = Vec::new();
    if pipeline.allows_tools() {
        #[cfg(target_os = "macos")]
        match with_sips(&path, size, full) {
            Ok(image) => return Ok(image),
            Err(error) => errors.push(error),
        }
        for program in ["magick", "convert"] {
            let mut command = Command::new(program);
            let mut first_frame = path.as_os_str().to_os_string();
            first_frame.push("[0]");
            command.arg(first_frame).arg("-auto-orient");
            if !full {
                command.arg("-resize").arg(format!("{size}x{size}>"));
            }
            command.args(["-depth", "8", "png:-"]);
            let result = command
                .output()
                .map_err(|e| format!("{program}: {e}"))
                .and_then(|output| {
                    if !output.status.success() {
                        return Err(format!(
                            "{program}: {}",
                            String::from_utf8_lossy(&output.stderr).trim()
                        ));
                    }
                    image::load_from_memory_with_format(&output.stdout, image::ImageFormat::Png)
                        .map_err(|e| format!("{program} returned invalid pixels: {e}"))
                });
            match result {
                Ok(image) => return Ok(image.to_rgb8().into()),
                Err(error) => errors.push(error),
            }
        }
    }
    if !full && is_raw(&path) {
        match embedded_preview(&path, size) {
            Ok(image) => return Ok(image),
            Err(error) => errors.push(error),
        }
    }
    let native = if is_raw(&path) {
        develop_raw(&path)
    } else {
        crate::shared::image::load_image(&path).map(|image| image.to_rgb8().into())
    };
    match native {
        Ok(image) => return Ok(image),
        Err(error) => errors.push(error),
    }
    Err(format!(
        "Failed to generate preview for {} with external and built-in decoders: {}",
        path.display(),
        errors.join("; ")
    ))
}

pub(crate) fn develop_raw(path: &Path) -> Result<DynamicImage, String> {
    let raw = rawler::decode_file(path).map_err(|e| format!("Rust RAW decoder: {e}"))?;
    let mut image = rawler::imgop::develop::RawDevelop::default()
        .develop_intermediate(&raw)
        .map_err(|e| format!("Rust RAW development: {e}"))?
        .to_dynamic_image()
        .ok_or("Rust RAW development returned invalid pixels")?;
    if let Some(orientation) =
        image::metadata::Orientation::from_exif(raw.orientation.to_u16() as u8)
    {
        image.apply_orientation(orientation);
    }
    Ok(image)
}

pub(crate) fn load_native(path: &Path) -> Result<DynamicImage, String> {
    if is_raw(path) {
        return develop_raw(path);
    }
    if super::image::is_heic_family(path) {
        return super::image::load_image(path);
    }
    let mut decoder = image::ImageReader::open(path)
        .map_err(|e| format!("Failed to open {}: {e}", path.display()))?
        .into_decoder()
        .map_err(|e| format!("Failed to decode {}: {e}", path.display()))?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Ok(image)
}

#[cfg(target_os = "macos")]
pub(crate) fn with_sips(path: &Path, size: u32, full: bool) -> Result<DynamicImage, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // An exclusively created directory isolates concurrent conversions and is
    // removed on every exit, including failures in the external decoder.
    let directory = loop {
        let candidate = std::env::temp_dir().join(format!(
            "still-raw-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("sips temporary directory: {e}")),
        }
    };
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let directory = Cleanup(directory);
    let output = directory.0.join("preview.png");
    let mut command = Command::new("sips");
    command.args(["-s", "format", "png"]);
    if !full {
        command
            .arg("--resampleHeightWidthMax")
            .arg(size.to_string());
    }
    let result = command
        .arg(path)
        .arg("--out")
        .arg(&output)
        .output()
        .map_err(|e| format!("sips: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "sips: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let mut decoder = image::ImageReader::open(output)
        .map_err(|e| format!("sips output: {e}"))?
        .into_decoder()
        .map_err(|e| format!("sips output: {e}"))?;
    let orientation = decoder
        .orientation()
        .map_err(|e| format!("sips orientation: {e}"))?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| format!("sips output: {e}"))?;
    image.apply_orientation(orientation);
    Ok(image.to_rgb8().into())
}

fn embedded_preview(path: &Path, size: u32) -> Result<DynamicImage, String> {
    let data = fs::read(path).map_err(|e| format!("Cannot read RAW file: {e}"))?;
    decode_embedded(data, size)
}

fn decode_embedded(data: Vec<u8>, size: u32) -> Result<DynamicImage, String> {
    let mut image = largest_jpeg(&data, size)?;
    let orientation = exif::Reader::new()
        .read_raw(data)
        .ok()
        .and_then(|exif| {
            exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|field| field.value.get_uint(0))
        })
        .and_then(|value| u8::try_from(value).ok())
        .and_then(image::metadata::Orientation::from_exif);
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    Ok(image)
}

fn largest_jpeg(data: &[u8], size: u32) -> Result<DynamicImage, String> {
    let mut candidates = Vec::new();
    for (offset, marker) in data.windows(3).enumerate() {
        if marker != [0xff, 0xd8, 0xff] {
            continue;
        }
        if let Ok(decoder) = image::codecs::jpeg::JpegDecoder::new(Cursor::new(&data[offset..])) {
            let (width, height) = decoder.dimensions();
            if width.max(height) >= size {
                candidates.push((u64::from(width) * u64::from(height), offset));
            }
        }
    }
    candidates.sort_unstable_by(|a, b| b.cmp(a));
    for (_, offset) in candidates {
        if let Ok(image) =
            image::load_from_memory_with_format(&data[offset..], image::ImageFormat::Jpeg)
        {
            return Ok(image);
        }
    }
    Err("No decodable embedded JPEG large enough for the requested preview".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_container_orientation_to_embedded_preview() {
        // Little-endian TIFF IFD0 containing Orientation=6 (90 degrees clockwise).
        let mut data = vec![
            0x49, 0x49, 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
        ];
        let mut jpeg = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(32, 16)
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        data.extend(jpeg.into_inner());
        let image = decode_embedded(data, 16).unwrap();
        assert_eq!((image.width(), image.height()), (16, 32));
    }

    #[test]
    fn recognizes_camera_formats() {
        for name in [
            "photo.CR2",
            "photo.cR3",
            "photo.NEF",
            "photo.ARW",
            "photo.RAF",
            "photo.DNG",
            "photo.ORF",
            "photo.RW2",
            "photo.PEF",
        ] {
            assert!(is_raw(Path::new(name)), "{name}");
        }
        for name in ["photo", "photo.jpg", "photo.heic"] {
            assert!(!is_raw(Path::new(name)));
        }
    }

    #[test]
    fn selects_largest_embedded_jpeg_and_rejects_insufficient_resolution() {
        let mut data = b"RAW container\xff\xd8\xff invalid jpeg".to_vec();
        for (width, height) in [(8, 4), (32, 16), (16, 8)] {
            let mut jpeg = Cursor::new(Vec::new());
            DynamicImage::new_rgb8(width, height)
                .write_to(&mut jpeg, image::ImageFormat::Jpeg)
                .unwrap();
            data.extend(jpeg.into_inner());
        }
        let image = largest_jpeg(&data, 16).unwrap();
        assert_eq!((image.width(), image.height()), (32, 16));
        assert!(largest_jpeg(&data, 64).is_err());
        assert!(largest_jpeg(b"invalid raw", 1).is_err());
    }
}
