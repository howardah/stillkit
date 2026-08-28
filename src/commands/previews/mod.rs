use ::image::ImageFormat;
use clap::{Arg, ArgAction, Command};
use indicatif::{ProgressBar, ProgressStyle};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod batch;
mod image;
#[cfg(test)]
mod tests;

use batch::{generate_previews, preview_size_label, prompt_existing_file_action};
use image::do_generate_preview;

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

    fn to_format(self) -> ImageFormat {
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
    quality: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExistingFileAction {
    Overwrite,
    Skip,
}

pub fn subcommand() -> Command {
    Command::new("previews")
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
            Arg::new("quality")
                .short('q')
                .long("quality")
                .help("JPEG/WebP quality from 0 (most compressed) to 100 (best quality)")
                .value_name("QUALITY")
                .value_parser(clap::value_parser!(u8).range(0..=100))
                .default_value("75"),
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
    let quality = *matches.get_one::<u8>("quality").unwrap();

    let config = PreviewConfig {
        input_dir: input_dir.to_path_buf(),
        output_dir,
        max_dimension,
        format,
        recursive,
        full,
        clear_metadata,
        quality,
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
            "Generating preview: {}, format: {}, quality: {}, output: {}",
            preview_size_label(config.max_dimension, config.full),
            config.format.extension(),
            config.quality,
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
            config.quality,
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
