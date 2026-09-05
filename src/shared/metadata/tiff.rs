use exif::{Field, In, Tag};
use std::io::Cursor;

// Rebuild the freshly encoded TIFF directories around the original compressed
// strips. This preserves pixel depth and avoids a second image decode/encode.
pub(super) fn copy(bytes: &[u8], metadata: &[Field]) -> Result<Vec<u8>, String> {
    let output = exif::Reader::new()
        .read_raw(bytes.to_vec())
        .map_err(|e| e.to_string())?;
    let offsets = output
        .get_field(Tag::StripOffsets, In::PRIMARY)
        .ok_or("TIFF has no strip offsets")?;
    let counts = output
        .get_field(Tag::StripByteCounts, In::PRIMARY)
        .ok_or("TIFF has no strip sizes")?;
    let mut strips = Vec::new();
    let mut index = 0;
    while let Some(offset) = offsets.value.get_uint(index) {
        let count = counts
            .value
            .get_uint(index)
            .ok_or("Missing TIFF strip size")?;
        let end = (offset as usize)
            .checked_add(count as usize)
            .ok_or("TIFF strip overflow")?;
        strips.push(
            bytes
                .get(offset as usize..end)
                .ok_or("TIFF strip outside file")?,
        );
        index += 1;
    }
    let mut writer = exif::experimental::Writer::new();
    for field in output.fields().filter(|field| {
        field.ifd_num == In::PRIMARY && !metadata.iter().any(|source| source.tag == field.tag)
    }) {
        writer.push_field(field);
    }
    for field in metadata {
        writer.push_field(field);
    }
    writer.set_strips(&strips, In::PRIMARY);
    let mut result = Cursor::new(Vec::new());
    writer
        .write(&mut result, output.little_endian())
        .map_err(|e| e.to_string())?;
    Ok(result.into_inner())
}
