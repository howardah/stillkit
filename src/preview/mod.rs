use clap::{Arg, ArgAction, Command};
use image::{DynamicImage, GenericImageView, ImageFormat};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
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
    full: bool,
    clear_metadata: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExistingFileAction {
    Overwrite,
    Skip,
}

pub fn subcommand() -> Command {
    Command::new("preview")
        .about("Generate preview images")
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
        .arg(
            Arg::new("full")
                .long("full")
                .help("Keep original dimensions and skip resizing")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("clear_metadata")
                .long("clear-metadata")
                .help("Strip EXIF/XMP/IPTC metadata from generated previews")
                .action(ArgAction::SetTrue),
        )
}

pub fn run(matches: &clap::ArgMatches) {
    let input_path = matches
        .get_one::<String>("input")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("Cannot determine current directory"));

    let output_dir_name = matches.get_one::<String>("output").unwrap();

    // Determine input type and output directory
    let (input_dir, output_dir, is_single_file) = if input_path.is_file() {
        let output_dir = input_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(output_dir_name);
        (
            input_path.parent().unwrap_or_else(|| Path::new(".")),
            output_dir,
            true,
        )
    } else {
        let output_dir = input_path.join(output_dir_name);
        (input_path.as_path(), output_dir, false)
    };

    let max_dimension: u32 = matches
        .get_one::<String>("max_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let format_str = matches.get_one::<String>("format").unwrap();
    let format = OutputFormat::from_str(format_str).unwrap_or(OutputFormat::Jpeg);

    let recursive = matches.get_flag("recursive");
    let full = matches.get_flag("full");
    let clear_metadata = matches.get_flag("clear_metadata");

    let config = PreviewConfig {
        input_dir: input_dir.to_path_buf(),
        output_dir,
        max_dimension,
        format,
        recursive,
        full,
        clear_metadata,
    };

    if is_single_file {
        // Handle single file directly
        let input_file = input_path;
        let relative_path = input_file
            .file_name()
            .unwrap_or_else(|| input_file.as_os_str());
        let output_path = config
            .output_dir
            .join(relative_path)
            .with_extension(config.format.extension());

        // Create output directory
        fs::create_dir_all(&config.output_dir)
            .map_err(|e| {
                eprintln!(
                    "Error creating directory {}: {}",
                    config.output_dir.display(),
                    e
                )
            })
            .ok();

        println!(
            "Generating preview: {}, format: {}, output: {}",
            preview_size_label(config.max_dimension, config.full),
            config.format.extension(),
            output_path.display()
        );

        let progress = ProgressBar::new(1);
        progress.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} {msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .expect("valid progress template")
            .progress_chars("=> "),
        );
        progress.set_message("Generating preview");

        if output_path.exists()
            && matches!(prompt_existing_file_action(1), ExistingFileAction::Skip)
        {
            progress.inc(1);
            progress.finish_with_message("Skipped existing preview");
            println!("Skipped existing file: {}", output_path.display());
            return;
        }

        match do_generate_preview(
            &input_file,
            &output_path,
            config.max_dimension,
            config.format,
            config.full,
            config.clear_metadata,
        ) {
            Ok(_) => {
                progress.inc(1);
                progress.finish_with_message("Generated 1 preview");
            }
            Err(e) => {
                progress.inc(1);
                progress.finish_with_message("Failed");
                eprintln!("Error: {}", e);
            }
        }
    } else {
        generate_previews(&config);
    }
}

/// Generate preview images based on configuration
fn generate_previews(config: &PreviewConfig) {
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
        "Generating previews: {} images, {}, format: {}, output: {}",
        image_paths.len(),
        preview_size_label(config.max_dimension, config.full),
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
        .map_init(
            || progress.clone(),
            |progress, path| {
                let relative_path = path.strip_prefix(&config.input_dir).unwrap_or(path);
                let mut output_path = config.output_dir.join(relative_path);

                // Create parent directories
                if let Some(parent) = output_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        progress.inc(1);
                        return Err(format!(
                            "Error creating directory {}: {}",
                            parent.display(),
                            e
                        ));
                    }
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

fn prompt_existing_file_action(existing_count: usize) -> ExistingFileAction {
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

fn parse_existing_file_action_input(input: &str) -> Option<ExistingFileAction> {
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

/// Generate a preview image with max dimension
fn do_generate_preview(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    format: OutputFormat,
    full: bool,
    clear_metadata: bool,
) -> Result<(), String> {
    let orientation_normalized =
        generate_preview_image(input_path, output_path, max_dimension, format, full)?;

    if !clear_metadata {
        copy_metadata_with_exiftool(input_path, output_path, orientation_normalized)?;
    }

    Ok(())
}

pub(crate) fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            return matches!(
                ext.to_ascii_lowercase().as_str(),
                "jpg"
                    | "jpeg"
                    | "png"
                    | "webp"
                    | "gif"
                    | "bmp"
                    | "tiff"
                    | "tif"
                    | "heic"
                    | "heif"
                    | "hif"
                    | "arw"
                    | "cr2"
                    | "nef"
                    | "raf"
                    | "dng"
            );
        })
        .unwrap_or(false)
}

pub(crate) fn load_image(path: &Path) -> Result<DynamicImage, String> {
    if has_heic_extension(path) {
        load_heic_image(path)
    } else {
        image::ImageReader::open(path)
            .map_err(|e| format!("Failed to open image {}: {}", path.display(), e))?
            .decode()
            .map_err(|e| format!("Failed to decode image {}: {}", path.display(), e))
    }
}

fn generate_preview_image(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    format: OutputFormat,
    full: bool,
) -> Result<bool, String> {
    // HEIC-family files benefit from ImageMagick's decoder and auto-orientation when available.
    if has_heic_extension(input_path)
        && try_generate_heic_preview_with_magick(input_path, output_path, max_dimension, full)?
    {
        return Ok(true);
    }

    let img = load_image(input_path)?;

    let resized: DynamicImage = if full {
        img
    } else {
        let (width, height) = img.dimensions();
        let (new_width, new_height) = calculate_dimensions(width, height, max_dimension);

        if new_width < width || new_height < height {
            img.resize_exact(new_width, new_height, image::imageops::FilterType::Lanczos3)
        } else {
            img
        }
    };

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

    Ok(false)
}

fn has_heic_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "heic" | "heif" | "hif"))
        .unwrap_or(false)
}

fn try_generate_heic_preview_with_magick(
    input_path: &Path,
    output_path: &Path,
    max_dimension: u32,
    full: bool,
) -> Result<bool, String> {
    let mut command = ProcessCommand::new("magick");
    command.arg(input_path).arg("-auto-orient").arg("-strip");

    if !full {
        command
            .arg("-resize")
            .arg(format!("{max_dimension}x{max_dimension}>"));
    }

    let output = match command.arg(output_path).output() {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(format!(
                "Failed to launch ImageMagick for {}: {}",
                input_path.display(),
                err
            ));
        }
    };

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "ImageMagick failed to convert {}: {}",
        input_path.display(),
        stderr.trim()
    ))
}

fn copy_metadata_with_exiftool(
    input_path: &Path,
    output_path: &Path,
    orientation_normalized: bool,
) -> Result<(), String> {
    let (width, height) = image::image_dimensions(output_path).map_err(|e| {
        format!(
            "Failed to read output dimensions for {}: {}",
            output_path.display(),
            e
        )
    })?;

    let mut command = ProcessCommand::new("exiftool");
    command
        .arg("-overwrite_original")
        .arg("-TagsFromFile")
        .arg(input_path)
        .arg("-EXIF:all")
        .arg("-XMP:all")
        .arg("-IPTC:all")
        .arg("-ICC_Profile")
        .arg(format!("-IFD0:ImageWidth={width}"))
        .arg(format!("-IFD0:ImageHeight={height}"))
        .arg(format!("-ExifImageWidth={width}"))
        .arg(format!("-ExifImageHeight={height}"));

    if orientation_normalized {
        command.arg("-Orientation#=1");
    }

    let output = match command.arg(output_path).output() {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "ExifTool is required to preserve metadata for {}. Install `exiftool` or rerun with --clear-metadata.",
                input_path.display()
            ));
        }
        Err(err) => {
            return Err(format!(
                "Failed to launch ExifTool for {}: {}",
                input_path.display(),
                err
            ));
        }
    };

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "ExifTool failed to copy metadata from {} to {}: {}",
        input_path.display(),
        output_path.display(),
        stderr.trim()
    ))
}

fn preview_size_label(max_dimension: u32, full: bool) -> String {
    if full {
        "full size".to_string()
    } else {
        return format!("max dimension: {max_dimension}");
    }
}

/// Load a HEIC image using the heic crate
fn load_heic_image(path: &Path) -> Result<DynamicImage, String> {
    use heic::{DecoderConfig, PixelLayout};

    let data = fs::read(path)
        .map_err(|e| format!("Failed to read HEIC file {}: {}", path.display(), e))?;

    // Best-effort fallback when no system HEIC decoder is available.
    let output = DecoderConfig::new()
        .decode(&data, PixelLayout::Rgba8)
        .map_err(|e| format!("Failed to decode HEIC image {}: {}", path.display(), e))?;

    let expected_bytes = output.width as usize * output.height as usize * 4;
    let actual_bytes = output.data.len();
    let rgba_image = image::RgbaImage::from_raw(output.width, output.height, output.data)
        .ok_or_else(|| {
            format!(
                "Failed to create image from HEIC data for {} (expected {} bytes, got {} bytes)",
                path.display(),
                expected_bytes,
                actual_bytes
            )
        })?;

    Ok(DynamicImage::ImageRgba8(rgba_image))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;

    fn magick_available() -> bool {
        ProcessCommand::new("magick")
            .arg("-version")
            .output()
            .is_ok()
    }

    fn exiftool_available() -> bool {
        ProcessCommand::new("exiftool").arg("-ver").output().is_ok()
    }

    fn identify_verbose(path: &Path) -> String {
        let output = ProcessCommand::new("identify")
            .arg("-verbose")
            .arg(path)
            .output()
            .expect("identify should run");

        assert!(
            output.status.success(),
            "identify should succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8(output.stdout).expect("identify output should be valid utf-8")
    }

    fn exiftool_dump(path: &Path) -> String {
        let output = ProcessCommand::new("exiftool")
            .arg("-a")
            .arg("-G1")
            .arg("-s")
            .arg(path)
            .output()
            .expect("exiftool should run");

        assert!(
            output.status.success(),
            "exiftool should succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8(output.stdout).expect("exiftool output should be valid utf-8")
    }

    #[test]
    fn parses_existing_file_action_input() {
        assert_eq!(
            parse_existing_file_action_input("o"),
            Some(ExistingFileAction::Overwrite)
        );
        assert_eq!(
            parse_existing_file_action_input("overwrite"),
            Some(ExistingFileAction::Overwrite)
        );
        assert_eq!(
            parse_existing_file_action_input("s"),
            Some(ExistingFileAction::Skip)
        );
        assert_eq!(
            parse_existing_file_action_input(""),
            Some(ExistingFileAction::Skip)
        );
        assert_eq!(parse_existing_file_action_input("later"), None);
    }

    #[test]
    fn supports_hif_as_heic_family_image() {
        assert!(is_supported_image(Path::new("photo.hif")));
        assert!(is_supported_image(Path::new("photo.HIF")));
        assert!(has_heic_extension(Path::new("photo.hif")));
    }

    #[test]
    fn previews_heic_without_whiteout_when_magick_is_available() {
        if !magick_available() {
            return;
        }

        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/DSCF1164.HEIC");
        let output_path =
            std::env::temp_dir().join(format!("stillkit-preview-heic-{}.jpg", process::id()));
        let _ = fs::remove_file(&output_path);

        try_generate_heic_preview_with_magick(&path, &output_path, 1000, false)
            .expect("ImageMagick HEIC preview should succeed");

        let image = image::open(&output_path).expect("generated preview should be readable");
        let rgb = image.to_rgb8();

        let step_x = (rgb.width() / 16).max(1) as usize;
        let step_y = (rgb.height() / 16).max(1) as usize;
        let mut total = 0u64;
        let mut samples = 0u64;

        for y in (0..rgb.height() as usize).step_by(step_y) {
            for x in (0..rgb.width() as usize).step_by(step_x) {
                let pixel = rgb.get_pixel(x as u32, y as u32);
                total += u64::from(pixel[0]) + u64::from(pixel[1]) + u64::from(pixel[2]);
                samples += 3;
            }
        }

        let average = total as f64 / samples as f64;
        assert!(
            average < 220.0,
            "decoded HEIC output is unexpectedly blown out: {average}"
        );

        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn preview_size_label_reflects_full_mode() {
        assert_eq!(preview_size_label(1000, false), "max dimension: 1000");
        assert_eq!(preview_size_label(1000, true), "full size");
    }

    #[test]
    fn full_mode_keeps_original_dimensions() {
        let input_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/reference.jpg");
        let source = image::open(&input_path).expect("reference image should be readable");
        let output_path =
            std::env::temp_dir().join(format!("stillkit-preview-full-{}.png", process::id()));
        let _ = fs::remove_file(&output_path);

        do_generate_preview(&input_path, &output_path, 10, OutputFormat::Png, true, true)
            .expect("full preview generation should succeed");

        let output = image::open(&output_path).expect("generated image should be readable");
        assert_eq!(output.dimensions(), source.dimensions());

        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn preserves_metadata_by_default_when_magick_is_available() {
        if !exiftool_available() {
            return;
        }

        let input_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/reference.jpg");
        let output_path =
            std::env::temp_dir().join(format!("stillkit-preview-meta-{}.jpg", process::id()));
        let _ = fs::remove_file(&output_path);

        do_generate_preview(
            &input_path,
            &output_path,
            10,
            OutputFormat::Jpeg,
            false,
            false,
        )
        .expect("metadata-preserving preview generation should succeed");

        let report = identify_verbose(&output_path);
        assert!(report.contains("Profile-exif:"), "missing EXIF profile");
        assert!(report.contains("Profile-xmp:"), "missing XMP profile");
        assert!(
            report.contains("exif:Artist:"),
            "missing EXIF artist metadata"
        );
        assert!(
            report.contains("xmp:Rating:"),
            "missing XMP rating metadata"
        );
        assert!(report.contains("Filesize:"), "missing filesize metadata");

        let (width, height) = image::image_dimensions(&output_path)
            .expect("generated preview dimensions should be readable");
        let metadata = exiftool_dump(&output_path);
        assert!(
            metadata.contains(&format!(
                "[File]          ImageWidth                      : {width}"
            )),
            "file image width should match preview output"
        );
        assert!(
            metadata.contains(&format!(
                "[File]          ImageHeight                     : {height}"
            )),
            "file image height should match preview output"
        );
        assert!(
            metadata.contains(&format!(
                "[ExifIFD]       ExifImageWidth                  : {width}"
            )),
            "embedded EXIF width should match preview output"
        );
        assert!(
            metadata.contains(&format!(
                "[ExifIFD]       ExifImageHeight                 : {height}"
            )),
            "embedded EXIF height should match preview output"
        );

        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn clear_metadata_strips_profiles_when_magick_is_available() {
        if !exiftool_available() {
            return;
        }

        let input_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/reference.jpg");
        let output_path =
            std::env::temp_dir().join(format!("stillkit-preview-clear-meta-{}.jpg", process::id()));
        let _ = fs::remove_file(&output_path);

        do_generate_preview(
            &input_path,
            &output_path,
            10,
            OutputFormat::Jpeg,
            false,
            true,
        )
        .expect("metadata-cleared preview generation should succeed");

        let report = identify_verbose(&output_path);
        assert!(!report.contains("Profile-exif:"), "unexpected EXIF profile");
        assert!(!report.contains("Profile-xmp:"), "unexpected XMP profile");
        assert!(
            !report.contains("exif:Artist:"),
            "unexpected EXIF artist metadata"
        );
        assert!(
            !report.contains("xmp:Rating:"),
            "unexpected XMP rating metadata"
        );
        assert!(report.contains("Filesize:"), "missing filesize metadata");

        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn heic_metadata_dimensions_match_preview_output() {
        if !magick_available() || !exiftool_available() {
            return;
        }

        let input_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/DSCF1164.HEIC");
        let output_path =
            std::env::temp_dir().join(format!("stillkit-preview-heic-meta-{}.jpg", process::id()));
        let _ = fs::remove_file(&output_path);

        do_generate_preview(
            &input_path,
            &output_path,
            1000,
            OutputFormat::Jpeg,
            false,
            false,
        )
        .expect("HEIC preview generation should succeed");

        let (width, height) = image::image_dimensions(&output_path)
            .expect("generated preview dimensions should be readable");
        let metadata = exiftool_dump(&output_path);
        assert!(
            metadata.contains(&format!(
                "[File]          ImageWidth                      : {width}"
            )),
            "file image width should match preview output"
        );
        assert!(
            metadata.contains(&format!(
                "[File]          ImageHeight                     : {height}"
            )),
            "file image height should match preview output"
        );
        assert!(
            metadata.contains(&format!(
                "[ExifIFD]       ExifImageWidth                  : {width}"
            )),
            "embedded EXIF width should match preview output"
        );
        assert!(
            metadata.contains(&format!(
                "[ExifIFD]       ExifImageHeight                 : {height}"
            )),
            "embedded EXIF height should match preview output"
        );
        assert!(
            metadata.contains("[XMP-xmp]       Rating                          : 0"),
            "XMP rating should be preserved"
        );

        let _ = fs::remove_file(output_path);
    }
}
