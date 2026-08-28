use super::entry::{ImageEntry, build_rated_file_name, rename_with_rating, split_rating_suffix};
use super::import::{
    build_rating_index, collect_import_paths, image_entry_for_path, normalized_name_key,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn parses_rated_stems() {
    assert_eq!(split_rating_suffix("fish"), ("fish".to_string(), None));
    assert_eq!(
        split_rating_suffix("fish_★☆☆☆☆"),
        ("fish".to_string(), Some(1))
    );
    assert_eq!(
        split_rating_suffix("fish_☆☆☆☆☆"),
        ("fish".to_string(), Some(0))
    );
}

#[test]
fn keeps_original_title_for_display() {
    let entry = ImageEntry::from_path(
        Path::new("/photos"),
        PathBuf::from("/photos/fish_★★★☆☆.jpg"),
    );

    assert_eq!(entry.display_title, "fish.jpg");
    assert_eq!(entry.display_path, "fish.jpg");
    assert_eq!(entry.rating, Some(3));
}

#[test]
fn builds_rated_names() {
    assert_eq!(
        build_rated_file_name("fish", Some("jpg"), 1),
        "fish_★☆☆☆☆.jpg"
    );
    assert_eq!(
        build_rated_file_name("fish", Some("jpg"), 5),
        "fish_★★★★★.jpg"
    );
    assert_eq!(build_rated_file_name("fish", None, 0), "fish_☆☆☆☆☆");
}

#[test]
fn imports_rating_across_extensions() {
    let temp_root = temp_test_dir("import-cross-ext");
    let from_dir = temp_root.join("from");
    let to_dir = temp_root.join("to");
    fs::create_dir_all(&from_dir).expect("create from dir");
    fs::create_dir_all(&to_dir).expect("create to dir");

    let source = from_dir.join("DSCF0655_★☆☆☆☆.webp");
    let target = to_dir.join("DSCF0655.jpg");
    fs::write(&source, b"source").expect("write source");
    fs::write(&target, b"target").expect("write target");

    let (rating_index, _) =
        build_rating_index(&collect_import_paths(&from_dir).expect("collect source paths"));
    let entry = image_entry_for_path(target.clone());
    let rating = rating_index
        .get(&normalized_name_key(&entry.original_stem))
        .copied()
        .expect("rating should exist");

    rename_with_rating(&entry, rating).expect("rename target");

    assert!(!target.exists());
    assert!(to_dir.join("DSCF0655_★☆☆☆☆.jpg").exists());

    let _ = fs::remove_dir_all(temp_root);
}

#[test]
fn imports_rating_over_existing_target_rating() {
    let temp_root = temp_test_dir("import-overwrite-rating");
    let from_dir = temp_root.join("from");
    let to_dir = temp_root.join("to");
    fs::create_dir_all(&from_dir).expect("create from dir");
    fs::create_dir_all(&to_dir).expect("create to dir");

    let source = from_dir.join("DSCF0655_★☆☆☆☆.webp");
    let target = to_dir.join("DSCF0655_★★★☆☆.jpg");
    fs::write(&source, b"source").expect("write source");
    fs::write(&target, b"target").expect("write target");

    let (rating_index, _) =
        build_rating_index(&collect_import_paths(&from_dir).expect("collect source paths"));
    let entry = image_entry_for_path(target.clone());
    let rating = rating_index
        .get(&normalized_name_key(&entry.original_stem))
        .copied()
        .expect("rating should exist");

    rename_with_rating(&entry, rating).expect("rename target");

    assert!(!target.exists());
    assert!(to_dir.join("DSCF0655_★☆☆☆☆.jpg").exists());

    let _ = fs::remove_dir_all(temp_root);
}

fn temp_test_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("stillkit-rate-{label}-{}-{nanos}", process::id()))
}
