use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer, images, pixels::U8x3};
use image::{DynamicImage, GenericImageView, RgbImage};

#[cfg(test)]
mod benchmarks;

pub(super) fn preview(image: DynamicImage, max_dimension: u32, full: bool) -> DynamicImage {
    let (width, height) = image.dimensions();
    if full || width.max(height) <= max_dimension {
        return image;
    }
    let (width, height) = dimensions(width, height, max_dimension);
    if let DynamicImage::ImageRgb8(rgb) = &image
        && let Some(resized) = rgb_lanczos(rgb, width, height)
    {
        return DynamicImage::ImageRgb8(resized);
    }
    // Keep alpha, higher bit depths, and recoverable accelerator failures on the
    // existing path. Neither resizer changes the source's color space.
    image.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
}

fn rgb_lanczos(image: &RgbImage, width: u32, height: u32) -> Option<RgbImage> {
    if width == 0 || height == 0 || image.width() == 0 || image.height() == 0 {
        return None;
    }
    let source =
        images::TypedImageRef::<U8x3>::from_buffer(image.width(), image.height(), image.as_raw())
            .ok()?;
    let length = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(3)?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(length).ok()?;
    pixels.resize(length, 0);
    let mut destination =
        images::TypedImage::<U8x3>::from_buffer(width, height, &mut pixels).ok()?;
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    // Default construction selects supported SIMD at runtime (or scalar). Keep
    // scratch buffers local: retaining them per Rayon worker increases batch RSS.
    Resizer::new()
        .resize_typed(&source, &mut destination, &options)
        .ok()?;
    RgbImage::from_raw(width, height, pixels)
}

fn dimensions(width: u32, height: u32, max_dimension: u32) -> (u32, u32) {
    if width > height {
        let new_width = max_dimension;
        let new_height = (height as f32 * (new_width as f32 / width as f32)) as u32;
        (new_width, new_height.max(1))
    } else {
        let new_height = max_dimension;
        let new_width = (width as f32 * (new_height as f32 / height as f32)) as u32;
        (new_width.max(1), new_height)
    }
}

#[cfg(test)]
fn psnr(left: &RgbImage, right: &RgbImage) -> f64 {
    assert_eq!(left.dimensions(), right.dimensions());
    let mse = left
        .as_raw()
        .iter()
        .zip(right.as_raw())
        .map(|(&a, &b)| (f64::from(a) - f64::from(b)).powi(2))
        .sum::<f64>()
        / left.as_raw().len() as f64;
    10.0 * (255.0_f64.powi(2) / mse).log10()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma, Rgb, Rgba};

    #[test]
    fn constants_and_dimensions_are_preserved() {
        for (width, height, size, expected) in [
            (101, 67, 16, (16, 10)),
            (67, 101, 16, (10, 16)),
            (31, 31, 1, (1, 1)),
            (1, 101, 16, (1, 16)),
            (101, 1, 16, (16, 1)),
        ] {
            let source = RgbImage::from_pixel(width, height, Rgb([23, 121, 249]));
            let resized = preview(source.into(), size, false).into_rgb8();
            assert_eq!(resized.dimensions(), expected);
            assert!(resized.pixels().all(|pixel| pixel.0 == [23, 121, 249]));
        }
    }

    #[test]
    fn full_and_no_upscale_return_the_original_buffer() {
        for (size, full) in [(200, false), (101, false), (1, true)] {
            let source = RgbImage::new(101, 67);
            let pointer = source.as_raw().as_ptr();
            let resized = preview(source.into(), size, full).into_rgb8();
            assert_eq!(resized.dimensions(), (101, 67));
            assert_eq!(resized.as_raw().as_ptr(), pointer);
        }
    }

    #[test]
    fn gradients_and_edges_match_lanczos_quality() {
        for edges in [false, true] {
            let source = RgbImage::from_fn(257, 193, |x, y| {
                if edges {
                    Rgb([
                        if x < 128 { 0 } else { 255 },
                        if y < 96 { 255 } else { 0 },
                        63,
                    ])
                } else {
                    Rgb([(x * 255 / 256) as u8, (y * 255 / 192) as u8, 63])
                }
            });
            for (width, height) in [(100, 75), (17, 13), (1, 1)] {
                let expected = image::imageops::resize(
                    &source,
                    width,
                    height,
                    image::imageops::FilterType::Lanczos3,
                );
                let actual = rgb_lanczos(&source, width, height).unwrap();
                assert!(psnr(&expected, &actual) >= 40.0);
            }
        }
    }

    #[test]
    fn non_rgb_types_keep_the_existing_resizer() {
        let cases = [
            DynamicImage::ImageRgba8(ImageBuffer::from_fn(31, 19, |x, y| {
                Rgba([(x * 7) as u8, 83, (y * 11) as u8, (x * y % 256) as u8])
            })),
            DynamicImage::ImageLuma8(ImageBuffer::from_pixel(31, 19, Luma([97]))),
            DynamicImage::ImageRgb16(ImageBuffer::from_pixel(31, 19, Rgb([1, 32768, 65535]))),
            DynamicImage::ImageRgb32F(ImageBuffer::from_pixel(31, 19, Rgb([0.1, 0.5, 1.0]))),
        ];
        for source in cases {
            let expected = source.resize_exact(13, 7, image::imageops::FilterType::Lanczos3);
            let actual = preview(source, 13, false);
            assert_eq!(actual.color(), expected.color());
            assert_eq!(actual.as_bytes(), expected.as_bytes());
        }
    }

    #[test]
    fn invalid_accelerator_dimensions_are_recoverable() {
        assert!(rgb_lanczos(&RgbImage::new(4, 4), 0, 1).is_none());
        assert!(rgb_lanczos(&RgbImage::new(0, 0), 1, 1).is_none());
    }
}
