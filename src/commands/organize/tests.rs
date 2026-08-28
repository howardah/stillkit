use super::subcommand;

#[test]
fn organize_command_accepts_no_directory() {
    let matches = subcommand()
        .try_get_matches_from(["organize"])
        .expect("organize should be usable without a directory");

    assert!(matches.get_one::<String>("output").is_none());
    assert!(matches.get_one::<String>("input").is_none());
}

#[test]
fn organize_command_accepts_directory_path() {
    let matches = subcommand()
        .try_get_matches_from(["organize", "/photos"])
        .expect("organize should accept a directory path");

    assert_eq!(
        matches.get_one::<String>("output").map(String::as_str),
        Some("/photos")
    );
}
