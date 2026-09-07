use exif::{Context, Field, In, Tag, Value};
use img_parts::{DynImage, ImageEXIF};
use std::{fs, io::Cursor, path::Path, process::Command as ProcessCommand};

mod profiles;
mod tiff;

pub(crate) struct EncodedImage {
    pub(crate) bytes: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) fn copy_metadata(
    input: &Path,
    output: &Path,
    normalized: bool,
    pipeline: super::Pipeline,
    report_tool: super::ToolReporter<'_>,
) -> Result<(), String> {
    if pipeline.allows_tools()
        && copy_metadata_with_exiftool(input, output, normalized, report_tool).is_ok()
    {
        return Ok(());
    }
    copy_native(input, output, normalized)
        .map_err(|e| format!("Failed to copy metadata from {}: {e}", input.display()))
}

pub(crate) fn write_with_metadata(
    input: &Path,
    output: &Path,
    encoded: EncodedImage,
    normalized: bool,
    pipeline: super::Pipeline,
    report_tool: super::ToolReporter<'_>,
) -> Result<(), String> {
    if pipeline.allows_tools() {
        fs::write(output, &encoded.bytes)
            .map_err(|e| format!("Failed to write output file {}: {e}", output.display()))?;
        if copy_metadata_with_exiftool(input, output, normalized, report_tool).is_ok() {
            return Ok(());
        }
    }

    let encoded = copy_native_bytes(
        input,
        encoded.bytes,
        encoded.width,
        encoded.height,
        normalized,
    )
    .map_err(|e| format!("Failed to copy metadata from {}: {e}", input.display()))?;
    fs::write(output, encoded)
        .map_err(|e| format!("Failed to write output file {}: {e}", output.display()))
}

fn copy_native(input: &Path, output: &Path, normalized: bool) -> Result<(), String> {
    let (width, height) = image::image_dimensions(output).map_err(|e| e.to_string())?;
    let bytes = fs::read(output).map_err(|e| e.to_string())?;
    let bytes = copy_native_bytes(input, bytes, width, height, normalized)?;
    fs::write(output, bytes).map_err(|e| e.to_string())
}

fn copy_native_bytes(
    input: &Path,
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    normalized: bool,
) -> Result<Vec<u8>, String> {
    let data = fs::read(input).map_err(|e| e.to_string())?;
    let parsed = exif::Reader::new()
        .read_from_container(&mut Cursor::new(&data))
        .ok();
    let parsed = if parsed.is_none() && super::decoding::is_raw(input) {
        raw_exif(input).ok()
    } else {
        parsed
    };
    let mut fields: Vec<Field> = parsed
        .as_ref()
        .map(|exif| {
            exif.fields()
                .filter(|field| field.ifd_num == In::PRIMARY && portable_field(field))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for (tag, value) in [
        (Tag::ImageWidth, width),
        (Tag::ImageLength, height),
        (Tag::PixelXDimension, width),
        (Tag::PixelYDimension, height),
    ] {
        fields.retain(|field| field.tag != tag);
        fields.push(Field {
            tag,
            ifd_num: In::PRIMARY,
            value: Value::Long(vec![value]),
        });
    }
    if normalized {
        fields.retain(|field| field.tag != Tag::Orientation);
        fields.push(Field {
            tag: Tag::Orientation,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![1]),
        });
    }
    match image::guess_format(&bytes).map_err(|e| e.to_string())? {
        image::ImageFormat::Tiff => {
            for field in parsed
                .iter()
                .flat_map(|exif| exif.fields())
                .filter(|field| {
                    field.ifd_num == In::PRIMARY
                        && matches!(field.tag, Tag(Context::Tiff, 700 | 33723 | 34675))
                })
            {
                fields.push(field.clone());
            }
            let bytes = tiff::copy(&bytes, &fields)?;
            return Ok(bytes);
        }
        // These output formats do not support our photographic metadata containers.
        image::ImageFormat::Bmp | image::ImageFormat::Gif => return Ok(bytes),
        _ => {}
    }
    let mut writer = exif::experimental::Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut exif = Cursor::new(Vec::new());
    writer
        .write(
            &mut exif,
            parsed.as_ref().is_some_and(|exif| exif.little_endian()),
        )
        .map_err(|e| e.to_string())?;
    let mut container = DynImage::from_bytes(bytes.into())
        .map_err(|e| e.to_string())?
        .ok_or("Unsupported preview metadata container")?;
    let exif = exif.into_inner();
    if matches!(container, DynImage::Jpeg(_)) && exif.len() > 65_527 {
        return Err("EXIF metadata exceeds the JPEG segment limit".into());
    }
    container.set_exif(Some(exif.into()));
    profiles::copy(input, &data, &mut container)?;
    Ok(container.encoder().bytes().to_vec())
}

fn portable_field(field: &Field) -> bool {
    // Maker notes and RAW storage pointers cannot be relocated by a generic TIFF
    // writer. Preserve standard photographic EXIF/GPS, not invalid sensor offsets.
    if field.tag == Tag::MakerNote || matches!(field.value, Value::Unknown(..)) {
        return false;
    }
    match field.tag {
        Tag(Context::Tiff, id) => matches!(
            id,
            0x010e
                | 0x010f
                | 0x0110
                | 0x0112
                | 0x011a
                | 0x011b
                | 0x0128
                | 0x0131
                | 0x0132
                | 0x013b
                | 0x8298
                | 0x4746
                | 0x4749
                | 0x9c9b..=0x9c9f
        ),
        _ => true,
    }
}

fn raw_exif(input: &Path) -> Result<exif::Exif, String> {
    use rawler::{
        decoders::RawDecodeParams,
        formats::tiff::writer::{DirectoryWriter, TiffWriter},
        rawsource::RawSource,
        tags::TiffCommonTag,
    };
    let source = RawSource::new(input).map_err(|e| e.to_string())?;
    let decoder = rawler::get_decoder(&source).map_err(|e| e.to_string())?;
    let metadata = decoder
        .raw_metadata(&source, &RawDecodeParams::default())
        .map_err(|e| e.to_string())?;
    let mut data = Cursor::new(Vec::new());
    let mut writer = TiffWriter::new(&mut data).map_err(|e| e.to_string())?;
    let mut root = DirectoryWriter::new();
    let mut exif = DirectoryWriter::new();
    metadata
        .write_exif_tags(&mut writer, &mut root, &mut exif)
        .map_err(|e| e.to_string())?;
    root.add_tag(TiffCommonTag::Make, metadata.make.as_str());
    root.add_tag(TiffCommonTag::Model, metadata.model.as_str());
    root.add_tag(
        TiffCommonTag::ExifIFDPointer,
        exif.build(&mut writer).map_err(|e| e.to_string())?,
    );
    writer.build(root).map_err(|e| e.to_string())?;
    exif::Reader::new()
        .read_raw(data.into_inner())
        .map_err(|e| e.to_string())
}

fn copy_metadata_with_exiftool(
    input_path: &Path,
    output_path: &Path,
    orientation_normalized: bool,
    report_tool: super::ToolReporter<'_>,
) -> Result<(), String> {
    let (width, height) = image::image_dimensions(output_path).map_err(|e| {
        format!(
            "Failed to read output dimensions for {}: {}",
            output_path.display(),
            e
        )
    })?;

    report_tool("exiftool");
    let mut command = ProcessCommand::new("exiftool");
    command
        .arg("-overwrite_original")
        .arg("-TagsFromFile")
        .arg(input_path)
        .arg("-EXIF:all")
        .arg("-XMP:all")
        .arg("-IPTC:all")
        .arg("-ICC_Profile")
        .arg(format!("-IFD0:ImageWidth={width}"))
        .arg(format!("-IFD0:ImageHeight={height}"))
        .arg(format!("-ExifImageWidth={width}"))
        .arg(format!("-ExifImageHeight={height}"));

    if orientation_normalized {
        command.arg("-Orientation#=1");
    }

    let output = match command.arg(output_path).output() {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "ExifTool is unavailable for {}; use the built-in metadata writer.",
                input_path.display()
            ));
        }
        Err(err) => {
            return Err(format!(
                "Failed to launch ExifTool for {}: {}",
                input_path.display(),
                err
            ));
        }
    };

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "ExifTool failed to copy metadata from {} to {}: {}",
        input_path.display(),
        output_path.display(),
        stderr.trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageEncoder, RgbImage};

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
}
