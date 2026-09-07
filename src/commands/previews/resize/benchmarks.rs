//! Opt-in benchmarks: no camera file or timing assertion in the normal test suite.
use super::{psnr, rgb_lanczos};
use crate::commands::previews::{OutputFormat, embedded, image::encode_image};
use image::{DynamicImage, RgbImage};
use std::{hint::black_box, path::Path, time::Instant};

fn median<T>(mut operation: impl FnMut() -> T) -> f64 {
    drop(black_box(operation()));
    let mut samples = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let result = black_box(operation());
        samples.push(start.elapsed().as_secs_f64());
        drop(result);
    }
    samples.sort_by(f64::total_cmp);
    samples[2]
}

fn primary(data: &[u8]) -> RgbImage {
    let output = heic::DecoderConfig::new()
        .decode(data, heic::PixelLayout::Rgb8)
        .unwrap();
    RgbImage::from_raw(output.width, output.height, output.data).unwrap()
}

#[test]
#[ignore = "requires demo/DSCF0656.HEIC; run in release mode with --nocapture"]
fn benchmark_camera_preview_stages() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/DSCF0656.HEIC");
    let data = std::fs::read(path).expect("camera sample must be available");
    println!(
        "SIMD: {:?}",
        fast_image_resize::Resizer::new().cpu_extensions()
    );
    println!(
        "primary decode+RGB median: {:.6}s",
        median(|| primary(&data))
    );
    println!(
        "embedded decode median: {:.6}s",
        median(|| embedded::decode(&data, 1000).unwrap())
    );
    let primary = primary(&data);
    let embedded = embedded::decode(&data, 1000).unwrap().into_rgb8();
    for (label, source) in [("primary", &primary), ("embedded", &embedded)] {
        for size in [1000, 2000] {
            if size >= source.width().max(source.height()) {
                continue;
            }
            let (width, height) = super::dimensions(source.width(), source.height(), size);
            let old = || {
                image::imageops::resize(
                    source,
                    width,
                    height,
                    image::imageops::FilterType::Lanczos3,
                )
            };
            let new = || rgb_lanczos(source, width, height).unwrap();
            let old_time = median(old);
            let new_time = median(new);
            let reference = old();
            let resized = new();
            let quality = psnr(&reference, &resized);
            assert!(quality >= 40.0, "{label} {size}px PSNR {quality}");
            let resized = DynamicImage::ImageRgb8(resized);
            let encode = median(|| encode_image(&resized, OutputFormat::Jpeg, 75).unwrap());
            println!(
                "{label} {size}px: old_resize={old_time:.6}s new_resize={new_time:.6}s jpeg={encode:.6}s PSNR={quality:.2}dB"
            );
            // Optional inspection outputs must live outside the source tree and
            // never replace an existing file. No output is written by default.
            if let Some(directory) = std::env::var_os("STILL_BENCH_OUTPUT") {
                let directory = Path::new(&directory);
                save_new(
                    directory,
                    &format!("{label}-{size}-before.png"),
                    reference.into(),
                );
                save_new(directory, &format!("{label}-{size}-after.png"), resized);
            }
        }
    }
}

fn save_new(directory: &Path, name: &str, image: DynamicImage) {
    let path = directory.join(name);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    image.write_to(&mut file, image::ImageFormat::Png).unwrap();
}
