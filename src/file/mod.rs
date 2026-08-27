use chrono::{Datelike, NaiveDate};
use clap::{Arg, ArgAction, ArgMatches, Command};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod date;
mod group;
mod layout;
mod ops;
mod scan;

use date::DateSource;
use group::{PhotoFile, cluster_by_date};
use layout::{day_dir_name, file_subdir, is_primary, month_dir_name};
use scan::{find_matching_dir, scan_month_dir};

struct PlannedBatch {
    dest_root: PathBuf,
    files: Vec<PhotoFile>,
    has_primaries: bool,
    check_duplicates: bool,
}

#[derive(Default)]
struct ExecutionStats {
    processed: u64,
    moved: u64,
    skipped: u64,
    failed: u64,
}

pub fn subcommand() -> Command {
    Command::new("organize")
        .about("Organize photos into a date-based directory hierarchy")
        .arg(
            Arg::new("output")
                .help("Output directory (defaults to current directory)")
                .value_name("OUTPUT"),
        )
        .arg(
            Arg::new("output_flag")
                .short('o')
                .long("output")
                .help("Output directory")
                .value_name("DIR")
                .conflicts_with("output"),
        )
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .help("Input directory to import from")
                .value_name("DIR"),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help("Recurse into subdirectories of the input directory")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry_run")
                .long("dry-run")
                .help("Print planned actions without making any changes")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("fast")
                .long("fast")
                .help("Use filesystem modified dates instead of EXIF metadata")
                .action(ArgAction::SetTrue),
        )
}

#[cfg(test)]
mod tests {
    use super::subcommand;

    #[test]
    fn organize_command_accepts_no_directory() {
        let matches = subcommand()
            .try_get_matches_from(["organize"])
            .expect("organize should be usable without a directory");

        assert!(matches.get_one::<String>("output").is_none());
        assert!(matches.get_one::<String>("input").is_none());
    }

    #[test]
    fn organize_command_accepts_directory_path() {
        let matches = subcommand()
            .try_get_matches_from(["organize", "/photos"])
            .expect("organize should accept a directory path");

        assert_eq!(
            matches.get_one::<String>("output").map(String::as_str),
            Some("/photos")
        );
    }
}

pub fn run(matches: &ArgMatches) {
    let output = matches
        .get_one::<String>("output")
        .or_else(|| matches.get_one::<String>("output_flag"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("Cannot determine current directory"));

    let input = matches.get_one::<String>("input").map(PathBuf::from);
    let recursive = matches.get_flag("recursive");
    let dry_run = matches.get_flag("dry_run");
    let date_source = if matches.get_flag("fast") {
        DateSource::Filesystem
    } else {
        DateSource::Metadata
    };

    if let Some(input_dir) = input {
        import_mode(&input_dir, &output, recursive, dry_run, date_source);
    } else {
        sort_mode(&output, dry_run, date_source);
    }
}

// ---------------------------------------------------------------------------
// Sort mode
// ---------------------------------------------------------------------------

fn sort_mode(output: &Path, dry_run: bool, date_source: DateSource) {
    let raw_paths = collect_files(output, false, "Scanning output directory");
    let files = extract_photo_files(raw_paths, "Extracting photo dates", date_source);

    if files.is_empty() {
        println!("No files to sort.");
        return;
    }

    let plan: Vec<PlannedBatch> = cluster_by_date(files)
        .into_iter()
        .map(|cluster| {
            let month_dir = output.join(month_dir_name(cluster.start));
            let day_dir = month_dir.join(day_dir_name(cluster.start, cluster.end));
            let has_primaries = any_primary(&cluster.files);

            PlannedBatch {
                dest_root: day_dir,
                files: cluster.files,
                has_primaries,
                check_duplicates: false,
            }
        })
        .collect();

    let stats = execute_plan(&plan, dry_run, "Organizing photos", date_source);
    print_summary(&stats, dry_run, "sort");
}

// ---------------------------------------------------------------------------
// Import mode
// ---------------------------------------------------------------------------

fn import_mode(
    input: &Path,
    output: &Path,
    recursive: bool,
    dry_run: bool,
    date_source: DateSource,
) {
    let raw_paths = collect_files(input, recursive, "Scanning input directory");
    let files = extract_photo_files(raw_paths, "Extracting photo dates", date_source);

    if files.is_empty() {
        println!("No files to import.");
        return;
    }

    // Group by (year, month)
    let mut by_month: HashMap<(i32, u32), Vec<PhotoFile>> = HashMap::new();
    for file in files {
        by_month
            .entry((file.date.year(), file.date.month()))
            .or_default()
            .push(file);
    }

    let month_progress = progress_bar(by_month.len() as u64, "Matching files to existing folders");
    let mut plan: Vec<PlannedBatch> = Vec::new();

    for ((year, month), month_files) in by_month {
        let sample = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
        let month_dir = output.join(month_dir_name(sample));
        month_progress.set_message(format!("Matching {}", month_dir_name(sample)));

        let existing = if month_dir.exists() {
            scan_month_dir(&month_dir, date_source)
        } else {
            Vec::new()
        };

        // Split files into those going to an existing dir vs. brand-new dirs
        let mut to_existing: HashMap<PathBuf, Vec<PhotoFile>> = HashMap::new();
        let mut new_files: Vec<PhotoFile> = Vec::new();

        for file in month_files {
            if let Some(dir) = find_matching_dir(&existing, file.date) {
                to_existing.entry(dir.path.clone()).or_default().push(file);
            } else {
                new_files.push(file);
            }
        }

        // --- Files merging into an existing day dir ---
        for (existing_path, files) in to_existing {
            let has_primaries = dir_has_primaries(&existing_path) || any_primary(&files);
            plan.push(PlannedBatch {
                dest_root: existing_path,
                files,
                has_primaries,
                check_duplicates: true,
            });
        }

        // --- Files going into newly created day dirs ---
        for cluster in cluster_by_date(new_files) {
            let day_dir = month_dir.join(day_dir_name(cluster.start, cluster.end));
            let has_primaries = any_primary(&cluster.files);
            plan.push(PlannedBatch {
                dest_root: day_dir,
                files: cluster.files,
                has_primaries,
                check_duplicates: false,
            });
        }

        month_progress.inc(1);
    }

    month_progress.finish_with_message("Matching complete");

    let stats = execute_plan(&plan, dry_run, "Organizing photos", date_source);
    print_summary(&stats, dry_run, "import");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn collect_files(dir: &Path, recursive: bool, label: &str) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let progress = spinner(label);

    if recursive {
        for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
            let path = entry.path().to_path_buf();
            if path.is_file() {
                result.push(path);
                progress.inc(1);
            }
        }
    } else if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                result.push(path);
                progress.inc(1);
            }
        }
    } else {
        progress.finish_and_clear();
        eprintln!("Error reading directory: {}", dir.display());
        return result;
    }

    progress.finish_with_message(format!("{label}: found {} files", result.len()));
    result
}

fn extract_photo_files(
    paths: Vec<PathBuf>,
    label: &str,
    date_source: DateSource,
) -> Vec<PhotoFile> {
    if paths.is_empty() {
        return Vec::new();
    }

    let progress = progress_bar(paths.len() as u64, label);
    let files: Vec<PhotoFile> = paths
        .into_par_iter()
        .filter_map(|path| {
            let extracted = date::extract_date(&path, date_source);
            progress.inc(1);

            match extracted {
                Some(d) => Some(PhotoFile { path, date: d }),
                None => {
                    progress.println(format!(
                        "Warning: no date found for {}, skipping",
                        path.display()
                    ));
                    None
                }
            }
        })
        .collect();

    progress.finish_with_message(format!("{label}: {} files ready", files.len()));
    files
}

fn execute_plan(
    plan: &[PlannedBatch],
    dry_run: bool,
    label: &str,
    date_source: DateSource,
) -> ExecutionStats {
    let total_files: u64 = plan.iter().map(|batch| batch.files.len() as u64).sum();
    let mut stats = ExecutionStats::default();

    if total_files == 0 {
        return stats;
    }

    let progress = if dry_run {
        println!("[DRY RUN] Would move:");
        None
    } else {
        Some(progress_bar(total_files, label))
    };

    for batch in plan {
        if let Some(progress) = progress.as_ref() {
            progress.set_message(format!("{label}: {}", display_name(&batch.dest_root)));
        }

        for file in &batch.files {
            let ext = ext_of(&file.path);
            let dest_dir = match file_subdir(ext, batch.has_primaries) {
                None => batch.dest_root.clone(),
                Some(sub) => batch.dest_root.join(sub),
            };

            let file_name = file.path.file_name().unwrap();
            let dest_path = dest_dir.join(file_name);

            if batch.check_duplicates && dest_path.exists() {
                if let Some(existing_date) = date::extract_date(&dest_path, date_source) {
                    if existing_date == file.date {
                        stats.processed += 1;
                        stats.skipped += 1;

                        if dry_run {
                            println!(
                                "[DRY RUN] Would skip:\n  {}  (same name and date found in {})\n",
                                file.path.display(),
                                display_name(&batch.dest_root)
                            );
                        } else if let Some(progress) = progress.as_ref() {
                            progress.println(format!(
                                "[SKIP] {}  (same name and date found in {})",
                                file.path.display(),
                                display_name(&batch.dest_root)
                            ));
                            progress.inc(1);
                        }

                        continue;
                    }
                }
            }

            if dry_run {
                println!("  {}\n    → {}\n", file.path.display(), dest_path.display());
            } else {
                match ops::move_file(&file.path, &dest_dir) {
                    Ok(()) => stats.moved += 1,
                    Err(err) => {
                        stats.failed += 1;
                        if let Some(progress) = progress.as_ref() {
                            progress.println(format!(
                                "Error moving {}: {}",
                                file.path.display(),
                                err
                            ));
                        }
                    }
                }
                if let Some(progress) = progress.as_ref() {
                    progress.inc(1);
                }
            }

            stats.processed += 1;
        }
    }

    if let Some(progress) = progress {
        progress.finish_with_message(format!("{label}: processed {} files", stats.processed));
    }

    stats
}

fn print_summary(stats: &ExecutionStats, dry_run: bool, mode: &str) {
    if dry_run {
        println!(
            "Dry run complete: planned {} files for {}.",
            stats.processed, mode
        );
        return;
    }

    if stats.failed == 0 && stats.skipped == 0 {
        println!("Completed {}: moved {} files.", mode, stats.moved);
        return;
    }

    println!(
        "Completed {}: moved {}, skipped {}, failed {}.",
        mode, stats.moved, stats.skipped, stats.failed
    );
}

fn spinner(label: &str) -> ProgressBar {
    let progress = ProgressBar::new_spinner();
    progress.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg} [{pos} files]")
            .expect("valid spinner template"),
    );
    progress.set_message(label.to_string());
    progress.enable_steady_tick(Duration::from_millis(100));
    progress
}

fn progress_bar(len: u64, label: &str) -> ProgressBar {
    let progress = ProgressBar::new(len);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} {msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
        )
        .expect("valid progress template")
        .progress_chars("=> "),
    );
    progress.set_message(label.to_string());
    progress
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn any_primary(files: &[PhotoFile]) -> bool {
    files.iter().any(|f| is_primary(ext_of(&f.path)))
}

fn dir_has_primaries(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let p = e.path();
        p.is_file() && is_primary(ext_of(&p))
    })
}

fn ext_of(path: &Path) -> &str {
    path.extension().and_then(|e| e.to_str()).unwrap_or("")
}
