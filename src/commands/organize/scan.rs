use super::date::{DateSource, extract_date};
use super::layout::{is_primary, is_raw};
use chrono::NaiveDate;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct ExistingDir {
    pub path: PathBuf,
    pub min_date: NaiveDate,
    pub max_date: NaiveDate,
    pub dates: HashSet<NaiveDate>,
}

pub fn scan_month_dir(month_dir: &Path, date_source: DateSource) -> Vec<ExistingDir> {
    let mut result = Vec::new();

    let Ok(entries) = std::fs::read_dir(month_dir) else {
        return result;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let mut dates: HashSet<NaiveDate> = HashSet::new();

        // Scan files in the day dir root
        collect_dates(&path, &mut dates, date_source);

        // Scan one level of subdirectories (RAW/, XMP/, etc.)
        if let Ok(subdirs) = std::fs::read_dir(&path) {
            for subentry in subdirs.flatten() {
                let subpath = subentry.path();
                if subpath.is_dir() {
                    collect_dates(&subpath, &mut dates, date_source);
                }
            }
        }

        if dates.is_empty() {
            continue;
        }

        let min_date = *dates.iter().min().unwrap();
        let max_date = *dates.iter().max().unwrap();

        result.push(ExistingDir {
            path,
            min_date,
            max_date,
            dates,
        });
    }

    result
}

fn collect_dates(dir: &Path, dates: &mut HashSet<NaiveDate>, date_source: DateSource) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if (is_primary(ext) || is_raw(ext))
                && let Some(date) = extract_date(&path, date_source)
            {
                dates.insert(date);
            }
        }
    }
}

pub fn find_matching_dir(dirs: &[ExistingDir], date: NaiveDate) -> Option<&ExistingDir> {
    dirs.iter()
        .find(|d| d.dates.contains(&date) || (d.min_date < date && date < d.max_date))
}
