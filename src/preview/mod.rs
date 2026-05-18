use clap::{Arg, ArgAction, Command};
use image::{DynamicImage, GenericImageView, ImageFormat};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Supported output image formats
#[derive(Debug, Clone, Copy, Default)]
enum OutputFormat {
    #[default]
    Jpeg,
    Png,
    WebP,
}

impl OutputFormat {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }

    fn to_format(&self) -> ImageFormat {
        match self {
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Png => ImageFormat::Png,
            Self::WebP => ImageFormat::WebP,
        }
    }

    fn extension(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
        }
    }
}

/// Configuration for preview generation
struct PreviewConfig {
    input_dir: PathBuf,
    output_dir: PathBuf,
    max_dimension: u32,
    format: OutputFormat,
    recursive: bool,
}

pub fn subcommand() -> Command {
    Command::new("preview")
        .about("Generate preview images with a maximum dimension")
        .arg(
            Arg::new("input")
                .help("Input directory to process (defaults to current directory)")
                .index(1)
                .value_name("INPUT_DIR"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("Output directory name (defaults to 'preview')")
                .value_name("OUTPUT_DIR")
                .default_value("preview"),
        )
        .arg(
            Arg::new("max_size")
                .short('s')
                .long("max-size")
                .help("Maximum dimension in pixels (defaults to 1000)")
                .value_name("SIZE")
                .default_value("1000"),
        )
        .arg(
            Arg::new("format")
                .short('f')
                .long("format")
                .help("Output format: jpg, png, or webp (defaults to jpg)")
                .value_name("FORMAT")
                .default_value("jpg"),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help("Process subdirectories recursively")
                .action(ArgAction::SetTrue),
        )
}

pub fn run(matches: &clap::ArgMatches) {
    let input_dir = matches
        .get_one::<String>("input")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("Cannot determine current directory"));

    let output_dir_name = matches.get_one::<String>("output").unwrap();
    let output_dir = input_dir
        .parent()
        .unwrap_or(&input_dir)
        .join(output_dir_name);

    let max_dimension: u32 = matches
        .get_one::<String>("max_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let format_str = matches.get_one::<String>("format").unwrap();
    let format = OutputFormat::from_str(format_str).unwrap_or(OutputFormat::Jpeg);

    let recursive = matches.get_flag("recursive");

    let config = PreviewConfig {
        input_dir,
        output_dir,
        max_dimension,
        format,
        recursive,
    };

    generate_previews(&config);
}

/// Generate preview images based on configuration
fn generate_previews(config: &PreviewConfig) {
    // Collect all image files
    let image_paths = collect_image_files(&config.input_dir, config.recursive);

    if image_paths.is_empty() {
        println!("No image files found in {}.", config.input_dir.display());
        return;
    }

    println!(
        "Generating previews: {} images, max dimension: {}, format: {}, output: {}",
        image_paths.len(),
        config.max_dimension,
        config.format.extension(),
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
        .map(|path| {
            let relative_path = path.strip_prefix(&config.input_dir).unwrap_or(path);
            let mut output_path = config.output_dir.join(relative_path);

            // Create parent directories
            if let Some(parent) = output_path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return Err(format!(
                        "Error creating directory {}: {}",
                        parent.display(),
                        e
                    ));
                }
            }

            // Change extension to output format
            output_path = output_path.with_extension(config.format.extension());

            match do_generate_preview(path, &output_path, config.max_dimension, config.format) {
                Ok(_) => Ok(output_path),
                Err(e) => Err(e),
            }
        })
        .collect();

    // Update progress and handle errors
    let mut success_count = 0;
    let mut error_count = 0;

    for result in results {
        match result {
            Ok(_) => {
                success_count += 1;
            }
            Err(e) => {
                error_count += 1;
                progress.println(e);
            }
        }
        progress.inc(1);
    }

    progress.finish_with_message(format!(
        "Generated {} previews ({} errors)",
        success_count, error_count
    ));

    if error_count > 0 {
        eprintln!(
            "Encountered {} errors during preview generation.",
            error_count
        );
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

    let image_extensions: Vec<&str> = vec![
        "jpg", "jpeg", "png", "webp", "gif", "bmp", "tiff", "tif", "heic", "heif", "arw", "cr2",
        "nef", "raf", "dng",
    ];

    if recursive {
        for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if image_extensions
                        .iter()
                        .any(|&e| e.eq_ignore_ascii_case(ext))
                    {
                        result.push(path.to_path_buf());
                    }
                }
                progress.inc(1);
            }
        }
    } else if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if image_extensions
                        .iter()
                        .any(|&e| e.eq_ignore_ascii_case(ext))
                    {
                        result.push(path);
                    }
                }
                progress.inc(1);
            }
        }
    }

    progress.finish_with_message(format!("Found {} images", result.len()));
    result
}

/// Generate a preview image with max dimension
fn do_generate_preview(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    format: OutputFormat,
) -> Result<(), String> {
    // Open the image
    let img = image::ImageReader::open(input_path)
        .map_err(|e| format!("Failed to open image {}: {}", input_path.display(), e))?
        .decode()
        .map_err(|e| format!("Failed to decode image {}: {}", input_path.display(), e))?;

    // Calculate new dimensions maintaining aspect ratio
    let (width, height) = img.dimensions();
    let (new_width, new_height) = calculate_dimensions(width, height, max_dimension);

    // Resize if needed
    let resized: DynamicImage = if new_width < width || new_height < height {
        img.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    // Save with the specified format
    let mut bytes = Vec::new();
    resized
        .write_to(&mut Cursor::new(&mut bytes), format.to_format())
        .map_err(|e| format!("Failed to encode image {}: {}", input_path.display(), e))?;

    fs::write(output_path, &bytes).map_err(|e| {
        format!(
            "Failed to write output file {}: {}",
            output_path.display(),
            e
        )
    })?;

    Ok(())
}

/// Calculate new dimensions maintaining aspect ratio
fn calculate_dimensions(width: u32, height: u32, max_dimension: u32) -> (u32, u32) {
    if width > height {
        let new_width = max_dimension;
        let new_height = (height as f32 * (new_width as f32 / width as f32)) as u32;
        (new_width, new_height.max(1))
    } else {
        let new_height = max_dimension;
        let new_width = (width as f32 * (new_height as f32 / height as f32)) as u32;
        (new_width.max(1), new_height)
    }
}
