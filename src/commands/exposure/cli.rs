use clap::{Arg, ArgAction, Command, value_parser};

pub fn subcommand() -> Command {
    crate::shared::jxl::approval::arguments(Command::new("exposure")
        .about("Adjust image exposure")
        .arg(Arg::new("format").long("format").short('f').value_parser(["jxl"])
            .help("Convert output to JXL; otherwise retain the input format (HEIC/RAW use PNG)"))
        .arg(Arg::new("quality").long("quality").short('q')
            .value_parser(value_parser!(u8).range(0..=100)).default_value("90")
            .help("JXL quality, 0–100 (100 remains lossy; use --lossless for exact pixels)"))
        .arg(Arg::new("clear-metadata").long("clear-metadata")
            .help("Strip EXIF/XMP/IPTC metadata from output").action(ArgAction::SetTrue))
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
    )
}
