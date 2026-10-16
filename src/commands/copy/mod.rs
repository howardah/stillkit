use super::previews::{self, OutputFormat, PreviewOptions};
use crate::shared::{
    Pipeline,
    jxl::approval::{Approval, Policy},
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

pub(crate) fn subcommand() -> Command {
    let mut command =
        Command::new("copy").about("Convert exactly one image to one destination file");
    // Share refinement definitions so copy and previews accept the same controls.
    for arg in previews::subcommand().get_arguments() {
        if !matches!(arg.get_id().as_str(), "input" | "output" | "recursive") {
            command = command.arg(arg.clone());
        }
    }
    command
        .mut_arg("format", |arg| {
            arg.default_value(None::<&str>)
                .help("Output format (otherwise inferred from destination extension)")
        })
        .mut_arg("max_size", |arg| {
            arg.default_value("full")
                .help("Maximum dimension in pixels, or 'full' (default: full)")
        })
        .arg(
            Arg::new("source")
                .required(true)
                .index(1)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("destination")
                .required(true)
                .index(2)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("overwrite")
                .long("overwrite")
                .help("Replace an existing destination without prompting")
                .action(ArgAction::SetTrue),
        )
}

pub(crate) fn run(matches: &ArgMatches) -> bool {
    match execute(matches) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("{error}");
            false
        }
    }
}

fn inferred_format(path: &Path) -> Option<OutputFormat> {
    path.extension()
        .and_then(|s| s.to_str())
        .and_then(OutputFormat::from_str)
}

fn prompt(message: &str, choices: &[&str]) -> Result<usize, String> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(format!("{message}: an interactive terminal is required"));
    }
    loop {
        eprint!("{message} ");
        io::stderr()
            .flush()
            .map_err(|e| format!("Cannot show prompt: {e}"))?;
        let mut line = String::new();
        let count = io::stdin()
            .read_line(&mut line)
            .map_err(|e| format!("Cannot read answer: {e}"))?;
        if count == 0 || line.trim().is_empty() {
            return Ok(0);
        }
        if let Some(index) = choices
            .iter()
            .position(|s| *s == line.trim().to_ascii_lowercase())
        {
            return Ok(index);
        }
        eprintln!("Enter one of: {}", choices.join(", "));
    }
}

fn resolve_format(
    path: &Path,
    explicit: Option<OutputFormat>,
    dry_run: bool,
) -> Result<(PathBuf, OutputFormat), String> {
    let inferred = inferred_format(path);
    let format = explicit.or(inferred).ok_or_else(|| {
        format!(
            "Cannot infer output format from {}. Use --format jpg|png|webp|jxl.",
            path.display()
        )
    })?;
    if explicit.is_some() && inferred != Some(format) {
        let corrected = path.with_extension(format.extension());
        let message = format!(
            "Destination {} does not match --format {}. [c]ancel (default), [r]ename to {}, or [k]eep supplied path?",
            path.display(),
            format.extension(),
            corrected.display()
        );
        if dry_run {
            return Err(message);
        }
        match prompt(&message, &["c", "r", "k"])? {
            1 => return Ok((corrected, format)),
            2 => {}
            _ => return Err("Copy cancelled.".into()),
        }
    }
    Ok((path.to_path_buf(), format))
}

fn validate_paths(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!("Source is not a file: {}", source.display()));
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!(
            "Destination directory does not exist: {}. Create it first.",
            parent.display()
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(destination) {
        if !destination.is_file() {
            return Err(format!(
                "Destination is not a regular file: {}",
                destination.display()
            ));
        }
        let same = fs::canonicalize(source).ok() == fs::canonicalize(destination).ok();
        #[cfg(unix)]
        let same = {
            use std::os::unix::fs::MetadataExt;
            let source_meta = fs::metadata(source).map_err(|e| e.to_string())?;
            let target = fs::metadata(destination).unwrap_or(metadata);
            same || (source_meta.dev() == target.dev() && source_meta.ino() == target.ino())
        };
        #[cfg(not(unix))]
        let _ = metadata;
        if same {
            return Err(format!(
                "Destination refers to the source: {}",
                destination.display()
            ));
        }
    }
    Ok(())
}

fn execute(matches: &ArgMatches) -> Result<(), String> {
    let source = matches
        .get_one::<PathBuf>("source")
        .expect("required source");
    let destination = matches
        .get_one::<PathBuf>("destination")
        .expect("required destination");
    let policy = Policy::from_matches(matches);
    let size = matches.get_one::<String>("max_size").expect("default size");
    let full = matches.get_flag("full") || size.eq_ignore_ascii_case("full");
    let max_dimension = if size.eq_ignore_ascii_case("full") {
        1000
    } else {
        size.parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or("--max-size must be a positive integer or 'full'")?
    };
    let explicit = matches
        .get_one::<String>("format")
        .and_then(|s| OutputFormat::from_str(s));
    // Validate the supplied path before any interactive decisions, then the final path again.
    validate_paths(source, destination)?;
    let (destination, format) = resolve_format(destination, explicit, policy.dry_run)?;
    validate_paths(source, &destination)?;
    if matches.get_flag("lossless") && format != OutputFormat::Jxl {
        return Err("--lossless requires JXL output".into());
    }
    let mut overwrite = matches.get_flag("overwrite");
    if destination.exists() && !overwrite {
        let message = format!(
            "Destination exists: {}. Overwrite? [N/y]",
            destination.display()
        );
        if policy.dry_run {
            return Err(format!(
                "{message} Use --overwrite to authorize replacement."
            ));
        }
        if !matches!(prompt(&message, &["n", "y", "no", "yes"])?, 1 | 3) {
            return Err("Copy cancelled.".into());
        }
        overwrite = true;
    }
    let options = PreviewOptions {
        max_dimension,
        format,
        full,
        use_embedded: !matches.get_flag("no-embedded-preview"),
        clear_metadata: matches.get_flag("clear_metadata"),
        quality: *matches.get_one::<u8>("quality").expect("default quality"),
        lossless: matches.get_flag("lossless"),
        pipeline: match matches.get_one::<String>("tool").map(String::as_str) {
            Some("magick") => Pipeline::Magick,
            Some("sips") => Pipeline::Sips,
            _ => Pipeline::from_no_deps(matches.get_flag("no-deps")),
        },
    };
    // Approval and metadata handling must use the actual encoding even if the user keeps a mismatched suffix.
    if !Approval::new(policy).check(
        source,
        &destination.with_extension(format.extension()),
        options.clear_metadata,
    )? {
        return Err("Copy cancelled.".into());
    }
    if policy.dry_run {
        println!(
            "Would copy {} to {} (format: {}, size: {}, quality: {})",
            source.display(),
            destination.display(),
            format.extension(),
            if full {
                "full".into()
            } else {
                max_dimension.to_string()
            },
            options.quality
        );
        return Ok(());
    }
    convert(source, &destination, options, overwrite)?;
    println!("Copied {} to {}", source.display(), destination.display());
    Ok(())
}

fn convert(
    source: &Path,
    destination: &Path,
    options: PreviewOptions,
    overwrite: bool,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = loop {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = parent.join(format!(".still-copy-{}-{nonce}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => break path,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot stage output in {}: {e}", parent.display())),
        }
    };
    let result = (|| {
        let staged = staging
            .join("image")
            .with_extension(options.format.extension());
        previews::generate_preview(source, &staged, options, &|tool| {
            eprintln!("Converting [{tool}]")
        })?;
        validate_paths(source, destination)?;
        if overwrite {
            fs::rename(&staged, destination)
        } else {
            // Atomic no-clobber publication protects a destination created during conversion.
            fs::hard_link(&staged, destination)
        }
        .map_err(|e| format!("Cannot publish {}: {e}", destination.display()))
    })();
    let cleanup = fs::remove_dir_all(&staging);
    result?;
    cleanup.map_err(|e| format!("Cannot remove staging directory {}: {e}", staging.display()))
}
