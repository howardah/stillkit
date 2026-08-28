use super::entry::{ImageEntry, rename_with_rating};
use crate::shared::image::is_supported_image;
use clap::ArgMatches;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
struct ImportStats {
    updated: usize,
    unchanged: usize,
    missing_source: usize,
    source_unrated: usize,
    failed: usize,
}

pub(super) fn run_import(matches: &ArgMatches) -> Result<(), String> {
    let from = matches
        .get_one::<String>("from_flag")
        .or_else(|| matches.get_one::<String>("from"))
        .map(PathBuf::from)
        .ok_or_else(|| "missing --from/ FROM argument".to_string())?;
    let to = matches
        .get_one::<String>("to_flag")
        .or_else(|| matches.get_one::<String>("to"))
        .map(PathBuf::from)
        .ok_or_else(|| "missing --to/ TO argument".to_string())?;

    let source_files = collect_import_paths(&from)?;
    if source_files.is_empty() {
        return Err(format!("No image files found in {}", from.display()));
    }

    let target_files = collect_import_paths(&to)?;
    if target_files.is_empty() {
        return Err(format!("No image files found in {}", to.display()));
    }

    let (rating_index, unrated_source_count) = build_rating_index(&source_files);
    let mut stats = ImportStats::default();

    for target_path in target_files {
        let entry = image_entry_for_path(target_path.clone());
        let key = normalized_name_key(&entry.original_stem);

        let Some(source_rating) = rating_index.get(&key).copied() else {
            stats.missing_source += 1;
            continue;
        };

        if entry.rating == Some(source_rating) {
            stats.unchanged += 1;
            continue;
        }

        match rename_with_rating(&entry, source_rating) {
            Ok(_) => stats.updated += 1,
            Err(err) => {
                stats.failed += 1;
                eprintln!("{err}");
            }
        }
    }

    stats.source_unrated = unrated_source_count;

    println!(
        "Imported ratings: {} updated, {} unchanged, {} without source match, {} unrated sources ignored, {} failed",
        stats.updated, stats.unchanged, stats.missing_source, stats.source_unrated, stats.failed
    );

    Ok(())
}

pub(super) fn collect_import_paths(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut result = Vec::new();

    if path.is_file() {
        if is_supported_image(path) {
            result.push(path.to_path_buf());
        }
        return Ok(result);
    }

    if !path.is_dir() {
        return Err(format!("{} is not a file or directory", path.display()));
    }

    let entries = fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory {}: {}", path.display(), e))?;
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_file() && is_supported_image(&child) {
            result.push(child);
        }
    }

    result.sort_by(|a, b| {
        a.to_string_lossy()
            .to_lowercase()
            .cmp(&b.to_string_lossy().to_lowercase())
    });

    Ok(result)
}

pub(super) fn build_rating_index(paths: &[PathBuf]) -> (HashMap<String, u8>, usize) {
    let mut index = HashMap::new();
    let mut unrated = 0;

    for path in paths {
        let entry = image_entry_for_path(path.clone());
        let Some(rating) = entry.rating else {
            unrated += 1;
            continue;
        };

        index
            .entry(normalized_name_key(&entry.original_stem))
            .or_insert(rating);
    }

    (index, unrated)
}

pub(super) fn image_entry_for_path(path: PathBuf) -> ImageEntry {
    let root = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    ImageEntry::from_path(&root, path)
}

pub(super) fn normalized_name_key(name: &str) -> String {
    name.to_lowercase()
}
