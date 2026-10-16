const SIGNATURE: &[u8] = b"\0\0\0\x0cJXL \r\n\x87\n";

pub(crate) fn is_codestream(data: &[u8]) -> bool {
    data.starts_with(&[0xff, 0x0a]) || data.starts_with(SIGNATURE)
}

fn push_box(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Result<(), String> {
    let size = u32::try_from(data.len().checked_add(8).ok_or("JXL box is too large")?)
        .map_err(|_| "JXL box exceeds 4 GiB")?;
    output.extend_from_slice(&size.to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    Ok(())
}

pub(crate) fn add_metadata(
    bytes: Vec<u8>,
    exif: &[u8],
    xmp: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let mut output = SIGNATURE.to_vec();
    push_box(&mut output, b"ftyp", b"jxl \0\0\0\0jxl ")?;
    // Encoders emit a bare codestream when no metadata is supplied. Reject other
    // containers rather than creating nested containers or duplicate EXIF boxes.
    if !bytes.starts_with(&[0xff, 0x0a]) {
        return Err("Expected a bare JXL codestream before metadata attachment".into());
    }
    let mut payload = vec![0; 4]; // Offset to the TIFF header, relative to this field's end.
    payload.extend_from_slice(exif);
    push_box(&mut output, b"Exif", &payload)?;
    if let Some(xmp) = xmp {
        push_box(&mut output, b"xml ", xmp)?;
    }
    push_box(&mut output, b"jxlc", &bytes)?;
    Ok(output)
}
