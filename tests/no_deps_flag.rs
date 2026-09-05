#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, process::Command};
mod common;
use common::TestDir;

#[test]
fn no_deps_never_launches_installed_tools_for_single_files_or_batches() {
    let dir = TestDir::new();
    let tools = dir.0.join("tools");
    let input = dir.0.join("input");
    fs::create_dir(&tools).unwrap();
    fs::create_dir(&input).unwrap();
    let log = dir.0.join("calls");
    for program in ["sips", "magick", "convert", "exiftool", "identify"] {
        let path = tools.join(program);
        fs::write(
            &path,
            b"#!/bin/sh\nprintf '%s\\n' \"$0\" >> \"$STILL_TEST_LOG\"\nexit 1\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    image::DynamicImage::new_rgb8(32, 24)
        .save(input.join("photo.png"))
        .unwrap();
    common::dng(&input.join("camera.DNG"));
    for command in ["previews", "exposure"] {
        for batch in [false, true] {
            let output = dir.0.join(format!("{command}-{batch}"));
            let source = if batch {
                input.clone()
            } else {
                input.join("camera.DNG")
            };
            let mut process = Command::new(env!("CARGO_BIN_EXE_still"));
            process
                .env("PATH", &tools)
                .env("STILL_TEST_LOG", &log)
                .arg(command)
                .arg(&source)
                .arg("--no-deps")
                .arg("-o")
                .arg(&output);
            if command == "exposure" {
                process.args(["-e", "0", "--original-names"]);
            } else {
                process.arg("--full");
            }
            let result = process.output().unwrap();
            assert!(result.status.success());
            assert!(
                result.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                fs::read_dir(&output).unwrap().count(),
                if batch { 2 } else { 1 }
            );
            let image_path = output.join(if command == "previews" {
                "camera.jpg"
            } else {
                "camera.png"
            });
            let bytes = fs::read(image_path).unwrap();
            let exif = exif::Reader::new()
                .read_from_container(&mut std::io::Cursor::new(bytes))
                .unwrap();
            assert!(
                exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
                    .is_some()
            );
            assert!(!log.exists(), "--no-deps launched an external program");
        }
    }
}

#[test]
fn exposure_falls_back_from_magick_to_convert_then_rust() {
    let dir = TestDir::new();
    let input = dir.0.join("source.png");
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 1, image::Rgb([30, 40, 50])))
        .save(&input)
        .unwrap();
    let pixels = dir.0.join("converted.png");
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 1, image::Rgb([60, 80, 100])))
        .save(&pixels)
        .unwrap();
    let log = dir.0.join("calls");
    for program in ["magick", "convert"] {
        let path = dir.0.join(program);
        fs::write(&path, b"#!/bin/sh\nprintf '%s\\n' \"${0##*/}\" >> \"$STILL_TEST_LOG\"\nif [ \"${0##*/}\" = convert ] && [ \"$STILL_TEST_FAIL\" != yes ]; then for arg do output=\"$arg\"; done; /bin/cp \"$STILL_TEST_PIXELS\" \"$output\"; else exit 1; fi\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    for fail in ["no", "yes"] {
        let output = dir.0.join(fail);
        let result = Command::new(env!("CARGO_BIN_EXE_still"))
            .env("PATH", &dir.0)
            .env("STILL_TEST_LOG", &log)
            .env("STILL_TEST_PIXELS", &pixels)
            .env("STILL_TEST_FAIL", fail)
            .arg("exposure")
            .arg(&input)
            .args(["-e", "1", "-o"])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            image::open(output.join("source_+1_0.png"))
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0,
            [60, 80, 100]
        );
    }
    assert_eq!(
        fs::read_to_string(log).unwrap(),
        "magick\nconvert\nmagick\nconvert\n"
    );
}
