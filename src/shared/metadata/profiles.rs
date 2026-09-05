use image::ImageDecoder;
use img_parts::{
    Bytes, DynImage, ImageICC,
    jpeg::JpegSegment,
    png::PngChunk,
    riff::{RiffChunk, RiffContent},
};
use std::{io::Cursor, path::Path};

const XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

pub(super) fn copy(input: &Path, data: &[u8], target: &mut DynImage) -> Result<(), String> {
    let mut icc = None;
    let mut xmp = None;
    let mut iptc = None;
    if crate::shared::image::is_heic_family(input) {
        xmp = heic::DecoderConfig::new()
            .extract_xmp(data)
            .ok()
            .flatten()
            .map(|v| v.into_owned());
    } else if let Ok(reader) = image::ImageReader::new(Cursor::new(data)).with_guessed_format()
        && let Ok(mut decoder) = reader.into_decoder()
    {
        icc = decoder.icc_profile().ok().flatten();
        xmp = decoder.xmp_metadata().ok().flatten();
        iptc = if data.starts_with(&[0xff, 0xd8]) {
            jpeg_iptc(data)
        } else {
            decoder.iptc_metadata().ok().flatten()
        };
    }
    target.set_icc_profile(icc.map(Bytes::from));
    match target {
        DynImage::Jpeg(jpeg) => {
            if let Some(xmp) = xmp {
                let bytes = [XMP_PREFIX, &xmp].concat();
                if bytes.len() > 65_533 {
                    return Err("XMP exceeds the JPEG segment limit".into());
                }
                jpeg.segments_mut()
                    .insert(1, JpegSegment::new_with_contents(0xe1, bytes.into()));
            }
            // Keep complete Photoshop resource blocks, including IPTC datasets.
            if let Ok(Some(DynImage::Jpeg(source))) = DynImage::from_bytes(data.to_vec().into()) {
                for segment in source.segments().iter().filter(|s| s.marker() == 0xed) {
                    jpeg.segments_mut().insert(1, segment.clone());
                }
            } else if let Some(iptc) = iptc {
                let mut bytes = b"Photoshop 3.0\0\x38BIM\x04\x04\0\0".to_vec();
                bytes.extend_from_slice(&(iptc.len() as u32).to_be_bytes());
                bytes.extend_from_slice(&iptc);
                if iptc.len() % 2 != 0 {
                    bytes.push(0);
                }
                if bytes.len() > 65_533 {
                    return Err("IPTC exceeds the JPEG segment limit".into());
                }
                jpeg.segments_mut()
                    .insert(1, JpegSegment::new_with_contents(0xed, bytes.into()));
            }
        }
        DynImage::Png(png) => {
            if let Some(xmp) = xmp {
                let bytes = [b"XML:com.adobe.xmp\0\0\0\0\0".as_slice(), &xmp].concat();
                png.chunks_mut()
                    .insert(1, PngChunk::new(*b"iTXt", bytes.into()));
            }
            if let Some(iptc) = iptc {
                use std::fmt::Write;
                let mut text = format!("Raw profile type iptc\0\niptc\n{:8}\n", iptc.len());
                for (i, byte) in iptc.iter().enumerate() {
                    let _ = write!(text, "{byte:02x}");
                    if i % 36 == 35 {
                        text.push('\n');
                    }
                }
                text.push('\n');
                png.chunks_mut()
                    .insert(1, PngChunk::new(*b"tEXt", text.into_bytes().into()));
            }
        }
        DynImage::WebP(webp) => {
            if let Some(xmp) = xmp {
                webp.remove_chunks_by_id(*b"XMP ");
                webp.chunks_mut()
                    .push(RiffChunk::new(*b"XMP ", RiffContent::Data(xmp.into())));
            }
            // img-parts does not refresh existing VP8X flags after metadata edits.
            // Preserve alpha/animation bits while updating all metadata flags.
            let flags = (u8::from(webp.has_chunk(*b"ICCP")) * 0x20)
                | (u8::from(webp.has_chunk(*b"EXIF")) * 0x08)
                | (u8::from(webp.has_chunk(*b"XMP ")) * 0x04);
            if let Some(header) = webp
                .chunk_by_id(*b"VP8X")
                .and_then(|chunk| chunk.content().data())
            {
                let mut header = header.to_vec();
                if let Some(byte) = header.first_mut() {
                    *byte = (*byte & !0x2c) | flags;
                }
                webp.remove_chunks_by_id(*b"VP8X");
                webp.chunks_mut().insert(
                    0,
                    RiffChunk::new(*b"VP8X", RiffContent::Data(header.into())),
                );
            }
        }
    }
    Ok(())
}

fn jpeg_iptc(data: &[u8]) -> Option<Vec<u8>> {
    let DynImage::Jpeg(jpeg) = DynImage::from_bytes(data.to_vec().into()).ok()?? else {
        return None;
    };
    for segment in jpeg.segments().iter().filter(|s| s.marker() == 0xed) {
        let Some(mut resources) = segment.contents().strip_prefix(b"Photoshop 3.0\0") else {
            continue;
        };
        while resources.starts_with(b"8BIM") {
            let id = u16::from_be_bytes(resources.get(4..6)?.try_into().ok()?);
            let name_size = (usize::from(*resources.get(6)?) + 1).next_multiple_of(2);
            let size_offset = 6 + name_size;
            let size = u32::from_be_bytes(
                resources
                    .get(size_offset..size_offset + 4)?
                    .try_into()
                    .ok()?,
            ) as usize;
            let start = size_offset + 4;
            let payload = resources.get(start..start.checked_add(size)?)?;
            if id == 0x0404 {
                return Some(payload.to_vec());
            }
            resources = resources.get(start.checked_add(size.next_multiple_of(2))?..)?;
        }
    }
    None
}
