use chrono::{Datelike, NaiveDate};
use std::path::PathBuf;

pub struct PhotoFile {
    pub path: PathBuf,
    pub date: NaiveDate,
}

pub struct Cluster {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub files: Vec<PhotoFile>,
}

pub fn cluster_by_date(mut files: Vec<PhotoFile>) -> Vec<Cluster> {
    if files.is_empty() {
        return Vec::new();
    }

    files.sort_by_key(|f| f.date);

    let mut clusters: Vec<Cluster> = Vec::new();
    let mut current: Vec<PhotoFile> = Vec::new();

    for file in files {
        if current.is_empty() {
            current.push(file);
        } else {
            let last_date = current.last().unwrap().date;
            let gap = (file.date - last_date).num_days();
            let same_month = file.date.year() == last_date.year()
                && file.date.month() == last_date.month();

            if same_month && gap <= 2 {
                current.push(file);
            } else {
                clusters.push(finalize(current));
                current = vec![file];
            }
        }
    }

    if !current.is_empty() {
        clusters.push(finalize(current));
    }

    clusters
}

fn finalize(files: Vec<PhotoFile>) -> Cluster {
    let start = files.first().unwrap().date;
    let end = files.last().unwrap().date;
    Cluster { start, end, files }
}
