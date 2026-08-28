use clap::{Arg, ArgAction, ArgMatches, Command};
use std::path::PathBuf;

mod entry;
mod import;
mod preview;
#[cfg(test)]
mod tests;
mod ui;

use entry::collect_entries;
use import::run_import;
use ui::App;

pub fn subcommand() -> Command {
    Command::new("rate")
        .about("Rate images in a directory from 0-5 in a terminal UI")
        .arg(
            Arg::new("input")
                .help("Input directory to explore (defaults to current directory)")
                .index(1)
                .value_name("INPUT_DIR"),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help("Scan subdirectories recursively")
                .action(ArgAction::SetTrue),
        )
        .subcommand(import_subcommand())
}

pub fn run(matches: &ArgMatches) {
    let result = match matches.subcommand() {
        Some(("import", sub_matches)) => run_import(sub_matches),
        _ => run_inner(matches),
    };

    if let Err(err) = result {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn import_subcommand() -> Command {
    Command::new("import")
        .about("Import ratings from one file or directory into another")
        .arg(
            Arg::new("from_flag")
                .long("from")
                .value_name("FROM")
                .help("Source file or directory that already contains ratings")
                .conflicts_with("from"),
        )
        .arg(
            Arg::new("to_flag")
                .long("to")
                .value_name("TO")
                .help("Target file or directory to receive ratings")
                .conflicts_with("to"),
        )
        .arg(
            Arg::new("from")
                .index(1)
                .value_name("FROM")
                .help("Source file or directory that already contains ratings"),
        )
        .arg(
            Arg::new("to")
                .index(2)
                .value_name("TO")
                .help("Target file or directory to receive ratings"),
        )
}

fn run_inner(matches: &ArgMatches) -> Result<(), String> {
    let root = matches
        .get_one::<String>("input")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("Cannot determine current directory"));
    let recursive = matches.get_flag("recursive");

    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }

    let entries = collect_entries(&root, recursive)?;
    if entries.is_empty() {
        return Err(format!("No image files found in {}", root.display()));
    }

    let mut app = App::new(root, entries);
    ratatui::run(|terminal| app.run(terminal)).map_err(|e| format!("failed to run TUI: {e}"))?;
    Ok(())
}
