use crate::shared::image::{is_heic_family, is_supported_image};
use crate::shared::{Pipeline, decoding::is_raw};
use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
mod processing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputMode {
    Overwrite,
    NextToOriginal,
    Directory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NameMode {
    Original,
    Adjusted,
}

struct Job {
    input: PathBuf,
    output: PathBuf,
    adjustment: f64,
}

pub fn subcommand() -> Command {
    Command::new("exposure")
        .about("Adjust image exposure")
        .arg(
            Arg::new("no-deps")
                .long("no-deps")
                .help("Use built-in codecs and metadata handling; never launch external tools")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("inputs")
                .help("Image files or directories to process")
                .value_name("INPUT")
                .required(true)
                .num_args(1..),
        )
        .arg(
            Arg::new("adjustment")
                .short('e')
                .long("adjustment")
                .help("Exposure adjustment in stops, such as 1.5 or -0.2")
                .value_name("STOPS")
                .value_parser(value_parser!(f64))
                .allow_hyphen_values(true)
                .conflicts_with_all(["start", "end"]),
        )
        .arg(
            Arg::new("start")
                .long("start")
                .help("Starting exposure adjustment in stops for a ramp")
                .value_name("STOPS")
                .value_parser(value_parser!(f64))
                .allow_hyphen_values(true)
                .requires("end"),
        )
        .arg(
            Arg::new("end")
                .long("end")
                .help("Ending exposure adjustment in stops for a ramp")
                .value_name("STOPS")
                .value_parser(value_parser!(f64))
                .allow_hyphen_values(true)
                .requires("start"),
        )
        .arg(
            Arg::new("precision")
                .long("precision")
                .help("Decimal places in adjusted filenames (1 or 2; defaults to 1)")
                .value_name("PLACES")
                .value_parser(value_parser!(u8).range(1..=2))
                .default_value("1"),
        )
        .arg(
            Arg::new("overwrite")
                .long("overwrite")
                .help("Overwrite the input images in place")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["next-to-original", "output"]),
        )
        .arg(
            Arg::new("next-to-original")
                .long("next-to-original")
                .help("Save adjusted images beside their originals")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["overwrite", "output"]),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("Save images under this output directory")
                .value_name("DIRECTORY")
                .conflicts_with_all(["overwrite", "next-to-original"]),
        )
        .arg(
            Arg::new("original-names")
                .long("original-names")
                .help("When using --output, keep original filenames instead of adjustment suffixes")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help("Process directories recursively")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("force")
                .long("force")
                .help("Overwrite existing generated files in non-overwrite modes")
                .action(ArgAction::SetTrue),
        )
}

pub fn run(matches: &ArgMatches) {
    let inputs: Vec<PathBuf> = matches
        .get_many::<String>("inputs")
        .expect("inputs is required")
        .map(PathBuf::from)
        .collect();

    let adjustment = matches.get_one::<f64>("adjustment").copied();
    let start = matches.get_one::<f64>("start").copied();
    let end = matches.get_one::<f64>("end").copied();
    if adjustment.is_none() && (start.is_none() || end.is_none()) {
        eprintln!("Provide --adjustment or both --start and --end.");
        return;
    }

    let mode = if matches.get_flag("overwrite") {
        OutputMode::Overwrite
    } else if matches.get_flag("next-to-original") {
        OutputMode::NextToOriginal
    } else {
        OutputMode::Directory
    };

    let output_dir = matches.get_one::<String>("output").map(PathBuf::from);
    if mode == OutputMode::Directory && output_dir.is_none() {
        eprintln!("Provide --output DIRECTORY, or choose --overwrite / --next-to-original.");
        return;
    }
    if mode != OutputMode::Directory && matches.get_flag("original-names") {
        eprintln!("--original-names can only be used with --output.");
        return;
    }

    let images = collect_images(&inputs, matches.get_flag("recursive"));
    if images.is_empty() {
        eprintln!("No supported images found.");
        return;
    }

    let precision = *matches.get_one::<u8>("precision").unwrap_or(&1);
    let name_mode = if matches.get_flag("original-names") {
        NameMode::Original
    } else {
        NameMode::Adjusted
    };
    let adjustments = build_adjustments(images.len(), adjustment, start, end);
    let force = matches.get_flag("force");
    let pipeline = Pipeline::from_no_deps(matches.get_flag("no-deps"));
    let jobs = images
        .iter()
        .zip(&adjustments)
        .map(|(input, adjustment)| {
            processing::exposure_factor(*adjustment)?;
            let output = output_path(
                input,
                &inputs,
                mode,
                output_dir.as_deref(),
                name_mode,
                *adjustment,
                precision,
            )?;
            Ok(Job {
                input: input.clone(),
                output,
                adjustment: *adjustment,
            })
        })
        .collect::<Result<Vec<_>, String>>();
    let jobs = match jobs.and_then(|jobs| {
        validate_outputs(&jobs, mode)?;
        Ok(jobs)
    }) {
        Ok(jobs) => jobs,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };

    let progress = ProgressBar::new(images.len() as u64);
    progress.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg} [{bar:40.cyan/blue}] {pos}/{len}")
            .expect("valid progress template")
            .progress_chars("=> "),
    );
    progress.set_message("Adjusting exposure");

    let results: Vec<Result<PathBuf, String>> = jobs
        .par_iter()
        .map(|job| {
            let output = &job.output;

            if mode != OutputMode::Overwrite {
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
                }
                if output.exists() && !force {
                    return Err(format!(
                        "Output already exists: {} (use --force to replace it)",
                        output.display()
                    ));
                }
            }

            let result = processing::apply_exposure(
                &job.input,
                output,
                job.adjustment,
                pipeline,
                mode == OutputMode::Overwrite || force,
            );
            progress.inc(1);
            result.map(|_| output.clone())
        })
        .collect();

    let errors = results.iter().filter(|result| result.is_err()).count();
    for result in results {
        if let Err(error) = result {
            eprintln!("{error}");
        }
    }
    progress.finish_and_clear();
    println!(
        "Exposure complete: {} generated, {} errors",
        images.len() - errors,
        errors
    );
}

fn validate_outputs(jobs: &[Job], mode: OutputMode) -> Result<(), String> {
    let sources = jobs
        .iter()
        .map(|job| fs::canonicalize(&job.input))
        .collect::<Result<std::collections::HashSet<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut outputs = std::collections::HashSet::new();
    for job in jobs {
        let absolute = std::path::absolute(&job.output).map_err(|e| e.to_string())?;
        let key = fs::canonicalize(&absolute).unwrap_or_else(|_| {
            absolute
                .parent()
                .and_then(|parent| fs::canonicalize(parent).ok())
                .map(|parent| parent.join(absolute.file_name().unwrap_or_default()))
                .unwrap_or(absolute)
        });
        if mode != OutputMode::Overwrite && sources.contains(&key) {
            return Err(format!(
                "Output would replace an input: {}. Use a separate output directory or --overwrite.",
                job.output.display()
            ));
        }
        // Default macOS/Windows filesystems also collide on case-only changes.
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let key = PathBuf::from(key.as_os_str().to_ascii_lowercase());
        if !outputs.insert(key) {
            return Err(format!(
                "Multiple inputs produce {}. Process them separately or use distinct names.",
                job.output.display()
            ));
        }
    }
    Ok(())
}

fn collect_images(inputs: &[PathBuf], recursive: bool) -> Vec<PathBuf> {
    let mut images = Vec::new();
    for input in inputs {
        if input.is_file() {
            if is_supported_image(input) {
                images.push(input.clone());
            }
        } else if input.is_dir() {
            if recursive {
                for entry in walkdir::WalkDir::new(input).into_iter().flatten() {
                    if entry.path().is_file() && is_supported_image(entry.path()) {
                        images.push(entry.path().to_path_buf());
                    }
                }
            } else if let Ok(entries) = fs::read_dir(input) {
                for entry in entries.flatten() {
                    if entry.path().is_file() && is_supported_image(&entry.path()) {
                        images.push(entry.path());
                    }
                }
            }
        }
    }
    images.sort();
    images.dedup();
    images
}

fn build_adjustments(
    count: usize,
    adjustment: Option<f64>,
    start: Option<f64>,
    end: Option<f64>,
) -> Vec<f64> {
    if let Some(adjustment) = adjustment {
        return vec![adjustment; count];
    }
    let start = start.expect("start is required for a ramp");
    let end = end.expect("end is required for a ramp");
    if count <= 1 {
        return vec![start];
    }
    (0..count)
        .map(|index| start + (end - start) * index as f64 / (count - 1) as f64)
        .collect()
}

fn output_path(
    input: &Path,
    inputs: &[PathBuf],
    mode: OutputMode,
    output_dir: Option<&Path>,
    name_mode: NameMode,
    adjustment: f64,
    precision: u8,
) -> Result<PathBuf, String> {
    if mode == OutputMode::Overwrite {
        if is_heic_family(input) || is_raw(input) {
            return Err(format!(
                "Cannot overwrite HEIC/RAW in its original format: {}. Use --output or --next-to-original to save PNG copies.",
                input.display()
            ));
        }
        return Ok(input.to_path_buf());
    }

    let filename = input
        .file_name()
        .ok_or_else(|| format!("Input has no filename: {}", input.display()))?;
    let filename = if name_mode == NameMode::Adjusted {
        adjusted_filename(input, adjustment, precision)?
    } else {
        filename.to_os_string()
    };
    let filename = if is_heic_family(input) || is_raw(input) {
        PathBuf::from(filename)
            .with_extension("png")
            .into_os_string()
    } else {
        filename
    };

    if mode == OutputMode::NextToOriginal {
        return Ok(input.with_file_name(filename));
    }

    let output_dir = output_dir.expect("directory mode requires an output directory");
    let relative = common_relative_path(input, inputs);
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    Ok(output_dir.join(parent).join(filename))
}

fn common_relative_path(input: &Path, inputs: &[PathBuf]) -> PathBuf {
    for root in inputs {
        if root.is_dir()
            && let Ok(relative) = input.strip_prefix(root)
        {
            return relative.to_path_buf();
        }
    }
    input
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("image"))
}

fn adjusted_filename(
    input: &Path,
    adjustment: f64,
    precision: u8,
) -> Result<std::ffi::OsString, String> {
    let stem = input
        .file_stem()
        .ok_or_else(|| format!("Input has no filename: {}", input.display()))?;
    let extension = input.extension().unwrap_or(std::ffi::OsStr::new("jpg"));
    let scale = 10f64.powi(precision as i32);
    let rounded = (adjustment * scale).round() / scale;
    let value = format!("{rounded:.precision$}", precision = precision as usize)
        .replace('-', "-")
        .replace('.', "_");
    let value = if rounded >= 0.0 {
        format!("+{value}")
    } else {
        value
    };
    let mut filename = stem.to_os_string();
    filename.push(format!("_{value}."));
    filename.push(extension);
    Ok(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_deps_is_optional_and_preserves_output_conflicts() {
        let normal = subcommand()
            .try_get_matches_from(["exposure", "photo.png", "-e", "1", "--next-to-original"])
            .unwrap();
        assert!(!normal.get_flag("no-deps"));
        let native = subcommand()
            .try_get_matches_from([
                "exposure",
                "photo.png",
                "-e",
                "1",
                "--next-to-original",
                "--no-deps",
            ])
            .unwrap();
        assert!(native.get_flag("no-deps"));
        assert!(
            subcommand()
                .try_get_matches_from(["exposure", "--no-deps"])
                .is_err()
        );
        assert!(
            subcommand()
                .try_get_matches_from([
                    "exposure",
                    "photo.png",
                    "--no-deps",
                    "--overwrite",
                    "-o",
                    "output"
                ])
                .is_err()
        );
    }

    #[test]
    fn ramps_include_both_endpoints() {
        assert_eq!(
            build_adjustments(3, None, Some(-1.0), Some(1.0)),
            vec![-1.0, 0.0, 1.0]
        );
    }

    #[test]
    fn adjustment_filenames_round_to_requested_precision() {
        let path = adjusted_filename(Path::new("photo.jpg"), 1.456, 1).unwrap();
        assert_eq!(path, "photo_+1_5.jpg");
        let path = adjusted_filename(Path::new("photo.jpg"), -0.236, 2).unwrap();
        assert_eq!(path, "photo_-0_24.jpg");
    }
}
