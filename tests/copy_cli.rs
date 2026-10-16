use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "still-copy-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        image::RgbImage::new(12, 8)
            .save(root.join("source.png"))
            .unwrap();
        Self(root)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_still"))
            .current_dir(&self.0)
            .arg("copy")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn noninteractive_decisions_fail_and_overwrite_is_explicit() {
    let f = Fixture::new();
    fs::write(f.0.join("out.jpg"), b"original destination").unwrap();
    let result = f.run(&["source.png", "out.jpg"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("interactive terminal"));
    assert_eq!(
        fs::read(f.0.join("out.jpg")).unwrap(),
        b"original destination"
    );
    let result = f.run(&["source.png", "out.jpg", "--format", "png", "--overwrite"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("does not match"));
    let result = f.run(&["source.png", "out.jpg", "--overwrite", "--no-deps"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        image::image_dimensions(f.0.join("out.jpg")).unwrap(),
        (12, 8)
    );
}

#[test]
fn inferred_jxl_and_dry_run_work_through_public_command() {
    let f = Fixture::new();
    let result = f.run(&[
        "source.png",
        "out.JXL",
        "--lossless",
        "--no-deps",
        "--clear-metadata",
        "--dry-run",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!f.0.join("out.JXL").exists());
    let result = f.run(&[
        "source.png",
        "out.JXL",
        "--lossless",
        "--no-deps",
        "--clear-metadata",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(f.0.join("out.JXL").is_file());
    assert!(f.0.join("source.png").is_file());
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 2);
    let result = f.run(&["source.png", "source.png", "--overwrite"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("refers to the source"));
}
