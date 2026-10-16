use clap::{Arg, ArgAction, Command};

mod commands;
mod shared;

use commands::{classify, copy, exposure, organize, previews, rate};

fn cli() -> Command {
    Command::new("still")
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
        .subcommand(copy::subcommand())
        .subcommand(classify::subcommand())
        .subcommand(organize::subcommand())
        .subcommand(exposure::subcommand())
        .subcommand(previews::subcommand())
        .subcommand(rate::subcommand())
}

fn main() {
    let matches = cli().get_matches();

    match matches.subcommand() {
        Some(("classify", sub_m)) => classify::run(sub_m),
        Some(("organize", sub_m)) => organize::run(sub_m),
        Some(("exposure", sub_m)) => {
            if !exposure::run(sub_m) {
                std::process::exit(1);
            }
        }
        Some(("previews", sub_m)) => {
            if !previews::run(sub_m) {
                std::process::exit(1);
            }
        }
        Some(("copy", sub_m)) => {
            if !copy::run(sub_m) {
                std::process::exit(1);
            }
        }
        Some(("rate", sub_m)) => rate::run(sub_m),
        _ => {
            if matches.get_one::<String>("directory").is_some() {
                classify::run(&matches);
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

        let matches = cli()
            .try_get_matches_from([
                "still",
                "exposure",
                "photo.jpg",
                "--adjustment",
                "1.5",
                "--next-to-original",
            ])
            .expect("exposure should be a valid subcommand");
        assert_eq!(matches.subcommand_name(), Some("exposure"));
    }
}

#[cfg(test)]
mod jxl_cli_tests {
    use super::cli;
    #[test]
    fn quality_defaults_conflicts_and_lossless_forms() {
        for (command, default) in [("previews", 75), ("exposure", 90)] {
            let args = ["still", command, "photo.png", "--format", "jxl"];
            let matches = cli().try_get_matches_from(args).unwrap();
            let (_, matches) = matches.subcommand().unwrap();
            assert_eq!(matches.get_one::<u8>("quality"), Some(&default));
            assert!(
                cli()
                    .try_get_matches_from(args.into_iter().chain(["--lossless"]))
                    .is_ok()
            );
            assert!(
                cli()
                    .try_get_matches_from(args.into_iter().chain(["--lossless", "--quality", "90"]))
                    .is_err()
            );
            assert!(
                cli()
                    .try_get_matches_from(args.into_iter().chain(["--quality", "101"]))
                    .is_err()
            );
            assert!(
                cli()
                    .try_get_matches_from(args.into_iter().chain(["--quality", "0"]))
                    .is_ok()
            );
            assert!(
                cli()
                    .try_get_matches_from(args.into_iter().chain([
                        "--dry-run",
                        "--allow-iptc-loss",
                        "--clear-metadata"
                    ]))
                    .is_ok()
            );
        }
        assert!(cli().try_get_matches_from(["still", "exposure"]).is_err());
        assert!(
            cli()
                .try_get_matches_from(["still", "previews", "--format", "invalid"])
                .is_err()
        );
        assert!(cli().try_get_matches_from(["still", "photos"]).is_ok());
    }
}
