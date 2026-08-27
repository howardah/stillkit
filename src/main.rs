use clap::{Arg, ArgAction, Command};

mod file;
mod preview;
mod rate;
mod sort;

fn cli() -> Command {
    Command::new("pht")
        .about("Classify files and organize photos")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(false)
        // Top-level args kept for backwards compatibility with `still <directory>`.
        .arg(
            Arg::new("directory")
                .help("The directory to classify (extension classification mode)")
                .index(1),
        )
        .arg(
            Arg::new("extensions")
                .short('e')
                .long("extensions")
                .help("Custom directory names for extensions (e.g., 'raf:RAW')")
                .num_args(1..)
                .value_parser(clap::builder::ValueParser::string()),
        )
        .arg(
            Arg::new("ignore")
                .long("ignore")
                .help("Extensions to ignore (e.g., 'heic' or 'all')")
                .num_args(1..)
                .value_parser(clap::builder::ValueParser::string()),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help("Recursively classify files in subdirectories")
                .action(ArgAction::SetTrue),
        )
        .subcommand(sort::subcommand())
        .subcommand(file::subcommand())
        .subcommand(preview::subcommand())
        .subcommand(rate::subcommand())
}

fn main() {
    let matches = cli().get_matches();

    match matches.subcommand() {
        Some(("classify", sub_m)) => sort::run(sub_m),
        Some(("organize", sub_m)) => file::run(sub_m),
        Some(("previews", sub_m)) => preview::run(sub_m),
        Some(("rate", sub_m)) => rate::run(sub_m),
        _ => {
            if matches.get_one::<String>("directory").is_some() {
                sort::run(&matches);
            } else {
                eprintln!("No command or directory specified. Use --help for usage.");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::cli;

    #[test]
    fn exposes_the_renamed_subcommands() {
        let matches = cli()
            .try_get_matches_from(["still", "organize"])
            .expect("organize should be a valid subcommand");
        assert_eq!(matches.subcommand_name(), Some("organize"));

        let matches = cli()
            .try_get_matches_from(["still", "classify", "/photos"])
            .expect("classify should be a valid subcommand");
        assert_eq!(matches.subcommand_name(), Some("classify"));

        let matches = cli()
            .try_get_matches_from(["still", "previews", "/photos"])
            .expect("previews should be a valid subcommand");
        assert_eq!(matches.subcommand_name(), Some("previews"));
    }
}
