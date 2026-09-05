use exif::{Field, In, Tag, Value};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

pub struct TestDir(pub PathBuf);
impl TestDir {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "still-no-tools-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("Failed to create test directory: {e}"),
            }
        }
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn field(tag: Tag, value: Value) -> Field {
    Field {
        tag,
        value,
        ifd_num: In::PRIMARY,
    }
}

pub fn dng(path: &Path) {
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
