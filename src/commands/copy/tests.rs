use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "still-copy-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        image::RgbImage::new(1200, 20)
            .save(root.join("input.PNG"))
            .unwrap();
        Self(root)
    }
    fn args(&self, extra: &[&str]) -> ArgMatches {
        subcommand()
            .try_get_matches_from(
                std::iter::once("copy".into())
                    .chain([
                        self.0.join("input.PNG").into_os_string(),
                        self.0.join("output.png").into_os_string(),
                    ])
                    .chain(extra.iter().map(std::ffi::OsString::from)),
            )
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn parsing_requires_exactly_two_paths_and_preserves_refinement_controls() {
    for args in [
        vec!["copy"],
        vec!["copy", "a"],
        vec!["copy", "a", "b", "c"],
        vec!["copy", "a", "b", "--recursive"],
        vec!["copy", "a", "b", "--quality", "101"],
        vec!["copy", "a", "b", "--lossless", "--quality", "80"],
        vec!["copy", "a", "b", "--no-deps", "--tool", "sips"],
    ] {
        assert!(subcommand().try_get_matches_from(args).is_err());
    }
    let args = subcommand()
        .try_get_matches_from([
            "copy",
            "a",
            "b.jxl",
            "--lossless",
            "--clear-metadata",
            "--no-embedded-preview",
            "--no-deps",
            "--allow-iptc-loss",
            "--dry-run",
        ])
        .unwrap();
    assert_eq!(args.get_one::<u8>("quality"), Some(&75));
    assert_eq!(args.get_one::<String>("max_size").unwrap(), "full");
    assert!(args.get_one::<String>("format").is_none());
}

#[test]
fn inference_aliases_and_unresolved_mismatches() {
    for (name, expected) in [
        ("out.JPG", OutputFormat::Jpeg),
        ("out.jpeg", OutputFormat::Jpeg),
        ("out.PNG", OutputFormat::Png),
        ("out.WEBP", OutputFormat::WebP),
        ("out.JXL", OutputFormat::Jxl),
    ] {
        assert_eq!(
            resolve_format(Path::new(name), None, true).unwrap().1,
            expected
        );
    }
    for name in ["out", "out.unknown"] {
        assert!(resolve_format(Path::new(name), None, true).is_err());
        assert!(resolve_format(Path::new(name), Some(OutputFormat::Png), true).is_err());
    }
    assert!(resolve_format(Path::new("out.png"), Some(OutputFormat::Jxl), true).is_err());
    assert!(resolve_format(Path::new("out.jpeg"), Some(OutputFormat::Jpeg), true).is_ok());
}

#[test]
fn converts_full_size_and_resizes_only_when_requested() {
    let f = Fixture::new();
    let original = fs::read(f.0.join("input.PNG")).unwrap();
    execute(&f.args(&["--no-deps"])).unwrap();
    assert_eq!(
        image::image_dimensions(f.0.join("output.png")).unwrap(),
        (1200, 20)
    );
    execute(&f.args(&["--max-size", "100", "--overwrite", "--clear-metadata"])).unwrap();
    assert_eq!(
        image::image_dimensions(f.0.join("output.png")).unwrap(),
        (100, 1)
    );
    assert_eq!(fs::read(f.0.join("input.PNG")).unwrap(), original);
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 2);
}

#[test]
fn dry_run_and_invalid_sizes_leave_files_untouched() {
    let f = Fixture::new();
    execute(&f.args(&["--dry-run"])).unwrap();
    assert!(!f.0.join("output.png").exists());
    for size in ["0", "garbage", "-1"] {
        if let Ok(args) = subcommand().try_get_matches_from([
            "copy",
            f.0.join("input.PNG").to_str().unwrap(),
            f.0.join("output.png").to_str().unwrap(),
            "--max-size",
            size,
        ]) {
            assert!(execute(&args).is_err());
        }
    }
    fs::write(f.0.join("output.png"), b"existing").unwrap();
    assert!(execute(&f.args(&["--dry-run"])).is_err());
    execute(&f.args(&["--dry-run", "--overwrite"])).unwrap();
    assert_eq!(fs::read(f.0.join("output.png")).unwrap(), b"existing");
}

#[test]
fn invalid_paths_and_aliases_are_rejected() {
    let f = Fixture::new();
    let source = f.0.join("input.PNG");
    assert!(validate_paths(&source, &source).is_err());
    assert!(validate_paths(&f.0, &f.0.join("out.png")).is_err());
    assert!(validate_paths(&source, &f.0).is_err());
    assert!(validate_paths(&source, &f.0.join("missing/out.png")).is_err());
    fs::hard_link(&source, f.0.join("hard.png")).unwrap();
    assert!(validate_paths(&source, &f.0.join("hard.png")).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source, f.0.join("sym.png")).unwrap();
        assert!(validate_paths(&source, &f.0.join("sym.png")).is_err());
    }
}

#[test]
fn publication_does_not_clobber_and_encoding_uses_actual_format() {
    let f = Fixture::new();
    let source = f.0.join("input.PNG");
    let target = f.0.join("mismatch.jpg");
    let options = PreviewOptions {
        max_dimension: 100,
        format: OutputFormat::Png,
        full: false,
        use_embedded: true,
        clear_metadata: false,
        quality: 75,
        lossless: false,
        pipeline: Pipeline::BuiltIn,
    };
    convert(&source, &target, options, false).unwrap();
    assert_eq!(
        image::guess_format(&fs::read(&target).unwrap()).unwrap(),
        image::ImageFormat::Png
    );
    let original = fs::read(&target).unwrap();
    assert!(convert(&source, &target, options, false).is_err());
    assert_eq!(fs::read(&target).unwrap(), original);
    fs::write(&source, b"invalid image").unwrap();
    assert!(convert(&source, &target, options, true).is_err());
    assert_eq!(fs::read(&target).unwrap(), original);
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_convert_without_loss() {
    use std::os::unix::ffi::OsStringExt;
    let f = Fixture::new();
    let source =
        f.0.join(std::ffi::OsString::from_vec(b"in\xff.png".to_vec()));
    let output =
        f.0.join(std::ffi::OsString::from_vec(b"out\xfe.PNG".to_vec()));
    if let Err(error) = fs::rename(f.0.join("input.PNG"), &source) {
        if matches!(
            error.kind(),
            io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidInput
        ) {
            eprintln!(
                "Skipping non-UTF-8 filesystem test: platform rejected the filename: {error}"
            );
            return;
        }
        panic!("Cannot prepare non-UTF-8 source: {error}");
    }
    let matches = subcommand()
        .try_get_matches_from([
            std::ffi::OsString::from("copy"),
            source.into_os_string(),
            output.clone().into_os_string(),
        ])
        .unwrap();
    execute(&matches).unwrap();
    assert_eq!(image::image_dimensions(output).unwrap(), (1200, 20));
}
