use super::image::do_generate_preview;
use super::{ExistingFileAction, PreviewConfig};
use crate::shared::image::is_supported_image;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

/// Generate preview images based on configuration
pub(super) fn generate_previews(config: &PreviewConfig) {
    // Collect all image files
    let image_paths = collect_image_files(&config.input_dir, config.recursive);

    if image_paths.is_empty() {
        println!("No image files found in {}.", config.input_dir.display());
        return;
    }

    let existing_outputs = image_paths
        .iter()
        .filter(|path| preview_output_path(config, path).exists())
        .count();

    let existing_file_action = if existing_outputs > 0 {
        prompt_existing_file_action(existing_outputs)
    } else {
        ExistingFileAction::Overwrite
    };

    let image_paths: Vec<PathBuf> = if existing_file_action == ExistingFileAction::Skip {
        image_paths
            .into_iter()
            .filter(|path| !preview_output_path(config, path).exists())
            .collect()
    } else {
        image_paths
    };

    let skipped_count = if existing_file_action == ExistingFileAction::Skip {
        existing_outputs
    } else {
        0
    };

    if image_paths.is_empty() {
        println!(
            "All {} previews already exist. Skipping generation.",
            skipped_count
        );
        return;
    }

    println!(
        "Generating previews: {} images, {}, format: {}, quality: {}, output: {}",
        image_paths.len(),
        preview_size_label(config.max_dimension, config.full),
        config.format.extension(),
        config.quality,
        config.output_dir.display()
    );

    // Create progress bar
    let progress = ProgressBar::new(image_paths.len() as u64);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} {msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
        )
        .expect("valid progress template")
        .progress_chars("=> "),
    );
    progress.set_message("Generating previews");

    // Process images in parallel
    let results: Vec<Result<PathBuf, String>> = image_paths
        .par_iter()
        .map_init(
            || progress.clone(),
            |progress, path| {
                let relative_path = path.strip_prefix(&config.input_dir).unwrap_or(path);
                let mut output_path = config.output_dir.join(relative_path);

                // Create parent directories
                if let Some(parent) = output_path.parent()
                    && let Err(e) = fs::create_dir_all(parent)
                {
                    progress.inc(1);
                    return Err(format!(
                        "Error creating directory {}: {}",
                        parent.display(),
                        e
                    ));
                }

                // Change extension to output format
                output_path = output_path.with_extension(config.format.extension());

                let result = do_generate_preview(
                    path,
                    &output_path,
                    config.max_dimension,
                    config.format,
                    config.full,
                    config.clear_metadata,
                    config.quality,
                )
                .map(|_| output_path);
                progress.inc(1);
                result
            },
        )
        .collect();

    // Count errors and print them
    let error_count = results.iter().filter(|r| r.is_err()).count();
    for result in &results {
        if let Err(e) = result {
            progress.println(e);
        }
    }

    let success_count = image_paths.len() - error_count;

    let summary = if skipped_count > 0 {
        format!(
            "Generated {} previews ({} skipped, {} errors)",
            success_count, skipped_count, error_count
        )
    } else {
        format!(
            "Generated {} previews ({} errors)",
            success_count, error_count
        )
    };
    progress.finish_with_message(summary);

    if error_count > 0 {
        eprintln!(
            "Encountered {} errors during preview generation.",
            error_count
        );
    }
}

fn preview_output_path(config: &PreviewConfig, input_path: &Path) -> PathBuf {
    let relative_path = input_path
        .strip_prefix(&config.input_dir)
        .unwrap_or(input_path);
    config
        .output_dir
        .join(relative_path)
        .with_extension(config.format.extension())
}

pub(super) fn prompt_existing_file_action(existing_count: usize) -> ExistingFileAction {
    let noun = if existing_count == 1 {
        "preview"
    } else {
        "previews"
    };

    loop {
        print!(
            "{} existing {} detected. Overwrite existing files or skip them? [o/s] ",
            existing_count, noun
        );
        let _ = io::stdout().flush();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => match parse_existing_file_action_input(&input) {
                Some(action) => return action,
                None => {
                    eprintln!("Please enter 'o' to overwrite or 's' to skip.");
                }
            },
            Err(e) => {
                eprintln!("Failed to read input: {}. Skipping existing files.", e);
                return ExistingFileAction::Skip;
            }
        }
    }
}

pub(super) fn parse_existing_file_action_input(input: &str) -> Option<ExistingFileAction> {
    match input.trim().to_ascii_lowercase().as_str() {
        "o" | "overwrite" => Some(ExistingFileAction::Overwrite),
        "s" | "skip" | "" => Some(ExistingFileAction::Skip),
        _ => None,
    }
}

/// Collect all image files from directory
fn collect_image_files(dir: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let progress = ProgressBar::new_spinner();
    progress.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg} [{pos} files]")
            .expect("valid spinner template"),
    );
    progress.set_message("Scanning for images");
    progress.enable_steady_tick(Duration::from_millis(100));

    if recursive {
        for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
            let path = entry.path();
            if path.is_file() {
                if is_supported_image(path) {
                    result.push(path.to_path_buf());
                }
                progress.inc(1);
            }
        }
    } else if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if is_supported_image(&path) {
                    result.push(path);
                }
                progress.inc(1);
            }
        }
    }

    progress.finish_with_message(format!("Found {} images", result.len()));
    result
}

pub(super) fn preview_size_label(max_dimension: u32, full: bool) -> String {
    if full {
        "full size".to_string()
    } else {
        format!("max dimension: {max_dimension}")
    }
}
