use chrono::{DateTime, Local, NaiveDate};
use std::fs;
use std::io::BufReader;
use std::path::Path;

#[derive(Clone, Copy)]
pub enum DateSource {
    Metadata,
    Filesystem,
}

pub fn extract_date(path: &Path, source: DateSource) -> Option<NaiveDate> {
    match source {
        DateSource::Metadata => exif_date(path).or_else(|| fs_date(path)),
        DateSource::Filesystem => fs_date(path),
    }
}

fn exif_date(path: &Path) -> Option<NaiveDate> {
    let file = fs::File::open(path).ok()?;
    let mut buf = BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut buf).ok()?;

    let field = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))?;

    if let exif::Value::Ascii(ref vec) = field.value {
        let s: String = vec.first()?.iter().map(|&b| b as char).collect();
        return parse_exif_datetime(&s);
    }
    None
}

fn parse_exif_datetime(s: &str) -> Option<NaiveDate> {
    // EXIF format: "YYYY:MM:DD HH:MM:SS"
    let s = s.trim();
    if s.len() < 10 {
        return None;
    }
    let year: i32 = s[0..4].parse().ok()?;
    let month: u32 = s[5..7].parse().ok()?;
    let day: u32 = s[8..10].parse().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

fn fs_date(path: &Path) -> Option<NaiveDate> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let dt: DateTime<Local> = modified.into();
    Some(dt.date_naive())
}
