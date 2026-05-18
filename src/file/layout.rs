use chrono::{Datelike, NaiveDate};

pub fn month_dir_name(date: NaiveDate) -> String {
    format!(
        "{} - {:02} {}",
        date.year(),
        date.month(),
        month_name(date.month())
    )
}

pub fn day_dir_name(start: NaiveDate, end: NaiveDate) -> String {
    if start == end {
        start.format("%Y-%m-%d").to_string()
    } else {
        format!(
            "{} - {}",
            start.format("%Y-%m-%d"),
            end.format("%Y-%m-%d")
        )
    }
}

pub fn is_primary(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "jpg" | "jpeg" | "heic" | "mp4" | "mov"
    )
}

pub fn is_raw(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "raf" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "rw2" | "orf" | "pef" | "srw" | "x3f"
    )
}

/// Returns the subdirectory name a file should go into within a day dir,
/// or None if the file belongs in the day dir root.
pub fn file_subdir(ext: &str, has_primaries: bool) -> Option<String> {
    if ext.is_empty() || is_primary(ext) || !has_primaries {
        None
    } else if is_raw(ext) {
        Some("RAW".to_string())
    } else {
        Some(ext.to_uppercase())
    }
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => unreachable!(),
    }
}
