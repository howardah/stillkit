use crate::shared::image::is_supported_image;
use std::{
    fs,
    path::{Path, PathBuf},
};

const FILLED_STAR: char = '★';
const EMPTY_STAR: char = '☆';
pub(super) const MAX_RATING: u8 = 5;

pub(super) struct ImageEntry {
    pub(super) path: PathBuf,
    pub(super) display_path: String,
    pub(super) display_title: String,
    pub(super) original_stem: String,
    pub(super) extension: Option<String>,
    pub(super) rating: Option<u8>,
}

impl ImageEntry {
    pub(super) fn from_path(root: &Path, path: PathBuf) -> Self {
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        let (stem, extension) = split_file_name(&file_name);
        let (original_stem, rating) = split_rating_suffix(&stem);
        let display_title = build_display_title(&original_stem, extension.as_deref());

        let relative = path.strip_prefix(root).unwrap_or(&path);
        let display_path =
            if relative.parent().is_none() || relative.parent() == Some(Path::new("")) {
                display_title.clone()
            } else {
                let mut relative_display = relative.to_path_buf();
                relative_display.set_file_name(&display_title);
                relative_display.to_string_lossy().into_owned()
            };

        Self {
            path,
            display_path,
            display_title,
            original_stem,
            extension,
            rating,
        }
    }

    pub(super) fn rating_label(&self) -> String {
        self.rating
            .map(rating_to_stars)
            .unwrap_or_else(|| "unrated".to_string())
    }
}

pub(super) fn collect_entries(root: &Path, recursive: bool) -> Result<Vec<ImageEntry>, String> {
    let mut paths = Vec::new();

    if recursive {
        for entry in walkdir::WalkDir::new(root).into_iter().flatten() {
            let path = entry.path();
            if path.is_file() && is_supported_image(path) {
                paths.push(path.to_path_buf());
            }
        }
    } else {
        let entries = fs::read_dir(root)
            .map_err(|e| format!("Failed to read directory {}: {}", root.display(), e))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_supported_image(&path) {
                paths.push(path);
            }
        }
    }

    paths.sort_by(|a, b| {
        a.to_string_lossy()
            .to_lowercase()
            .cmp(&b.to_string_lossy().to_lowercase())
    });

    Ok(paths
        .into_iter()
        .map(|path| ImageEntry::from_path(root, path))
        .collect())
}

pub(super) fn rename_with_rating(entry: &ImageEntry, rating: u8) -> Result<PathBuf, String> {
    let new_name = build_rated_file_name(&entry.original_stem, entry.extension.as_deref(), rating);
    let new_path = entry.path.with_file_name(new_name);

    if new_path == entry.path {
        return Ok(new_path);
    }

    if new_path.exists() {
        return Err(format!(
            "Cannot rename {} because {} already exists",
            entry.path.display(),
            new_path.display()
        ));
    }

    fs::rename(&entry.path, &new_path).map_err(|e| {
        format!(
            "Failed to rename {} to {}: {}",
            entry.path.display(),
            new_path.display(),
            e
        )
    })?;

    Ok(new_path)
}

fn split_file_name(file_name: &str) -> (String, Option<String>) {
    match file_name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => {
            (stem.to_string(), Some(extension.to_string()))
        }
        _ => (file_name.to_string(), None),
    }
}

pub(super) fn split_rating_suffix(stem: &str) -> (String, Option<u8>) {
    let Some((base, suffix)) = stem.rsplit_once('_') else {
        return (stem.to_string(), None);
    };

    match parse_rating_suffix(suffix) {
        Some(rating) => (base.to_string(), Some(rating)),
        None => (stem.to_string(), None),
    }
}

fn parse_rating_suffix(suffix: &str) -> Option<u8> {
    if suffix.chars().count() != usize::from(MAX_RATING) {
        return None;
    }

    let mut filled = 0u8;
    for ch in suffix.chars() {
        match ch {
            FILLED_STAR => filled += 1,
            EMPTY_STAR => {}
            _ => return None,
        }
    }

    Some(filled)
}

fn build_display_title(stem: &str, extension: Option<&str>) -> String {
    match extension {
        Some(extension) => format!("{stem}.{extension}"),
        None => stem.to_string(),
    }
}

pub(super) fn build_rated_file_name(stem: &str, extension: Option<&str>, rating: u8) -> String {
    let rated_stem = format!("{stem}_{}", rating_to_stars(rating));
    build_display_title(&rated_stem, extension)
}

pub(super) fn rating_to_stars(rating: u8) -> String {
    let rating = rating.min(MAX_RATING);
    format!(
        "{}{}",
        FILLED_STAR.to_string().repeat(rating as usize),
        EMPTY_STAR
            .to_string()
            .repeat((MAX_RATING - rating) as usize)
    )
}
