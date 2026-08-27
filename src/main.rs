use clap::{Arg, ArgAction, Command};

mod file;
mod preview;
mod rate;
mod sort;

fn main() {
    let matches = Command::new("pht")
        .about("Sort files into directories")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(false)
        // Top-level args kept for backwards compatibility with `still <directory>`
        .arg(
            Arg::new("directory")
                .help("The directory to sort (extension sort mode)")
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
                .help("Recursively sort files in subdirectories")
                .action(ArgAction::SetTrue),
        )
        .subcommand(sort::subcommand())
        .subcommand(file::subcommand())
        .subcommand(preview::subcommand())
        .subcommand(rate::subcommand())
        .get_matches();

    match matches.subcommand() {
        Some(("sort", sub_m)) => sort::run(sub_m),
        Some(("photos", sub_m)) => file::run(sub_m),
        Some(("preview", sub_m)) => preview::run(sub_m),
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
