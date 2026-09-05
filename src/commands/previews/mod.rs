use ::image::ImageFormat;
use clap::{Arg, ArgAction, Command};
use std::path::{Path, PathBuf};

mod batch;
mod image;
use crate::shared::{Pipeline, decoding as raw};
#[cfg(test)]
mod tests;

use batch::generate_previews;
use image::PreviewOptions;

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
    inputs: PreviewInputs,
    output_dir: PathBuf,
    max_dimension: u32,
    format: OutputFormat,
    recursive: bool,
    full: bool,
    clear_metadata: bool,
    quality: u8,
    pipeline: Pipeline,
}

enum PreviewInputs {
    Directory(PathBuf),
    Files(Vec<PathBuf>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExistingFileAction {
    Overwrite,
    Skip,
}

impl PreviewConfig {
    fn image_options(&self) -> PreviewOptions {
        PreviewOptions {
            max_dimension: self.max_dimension,
            format: self.format,
            full: self.full,
            clear_metadata: self.clear_metadata,
            quality: self.quality,
            pipeline: self.pipeline,
        }
    }
}

pub fn subcommand() -> Command {
    Command::new("previews")
        .about("Generate preview images")
        .arg(
            Arg::new("no-deps")
                .long("no-deps")
                .help("Use built-in codecs and metadata handling; never launch external tools")
                .conflicts_with("tool")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("tool")
                .long("tool")
                .help("Force the RAW/HEIC conversion tool instead of using fallback order")
                .value_name("TOOL")
                .value_parser(["magick", "sips"]),
        )
        .arg(
            Arg::new("input")
                .help("Input directory or one or more image files (defaults to current directory)")
                .index(1)
                .value_name("INPUT")
                .num_args(1..),
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
                .help("Maximum dimension in pixels, or 'full' to keep original dimensions (defaults to 1000)")
                .value_name("SIZE|full")
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
    let input_paths: Vec<PathBuf> = matches
        .get_many::<String>("input")
        .map(|values| values.map(PathBuf::from).collect())
        .unwrap_or_else(|| {
            vec![std::env::current_dir().expect("Cannot determine current directory")]
        });

    let output_dir_name = matches.get_one::<String>("output").unwrap();

    let (inputs, output_dir) = if input_paths.len() == 1 && input_paths[0].is_dir() {
        let input_dir = input_paths.into_iter().next().unwrap();
        let output_dir = input_dir.join(output_dir_name);
        (PreviewInputs::Directory(input_dir), output_dir)
    } else {
        if let Some(path) = input_paths.iter().find(|path| !path.is_file()) {
            eprintln!("Input image is not a file: {}", path.display());
            return;
        }
        let output_dir = input_paths[0]
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(output_dir_name);
        (PreviewInputs::Files(input_paths), output_dir)
    };

    let max_size = matches.get_one::<String>("max_size").unwrap();
    let max_dimension = max_size.parse().unwrap_or(1000);

    let format_str = matches.get_one::<String>("format").unwrap();
    let format = OutputFormat::from_str(format_str).unwrap_or(OutputFormat::Jpeg);

    let recursive = matches.get_flag("recursive");
    let full = matches.get_flag("full") || max_size.eq_ignore_ascii_case("full");
    let clear_metadata = matches.get_flag("clear_metadata");
    let quality = *matches.get_one::<u8>("quality").unwrap();

    let config = PreviewConfig {
        inputs,
        output_dir,
        max_dimension,
        format,
        recursive,
        full,
        clear_metadata,
        quality,
        pipeline: match matches.get_one::<String>("tool").map(String::as_str) {
            Some("magick") => Pipeline::Magick,
            Some("sips") => Pipeline::Sips,
            _ => Pipeline::from_no_deps(matches.get_flag("no-deps")),
        },
    };

    generate_previews(&config);
}
