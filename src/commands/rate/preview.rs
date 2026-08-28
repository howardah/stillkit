use crate::shared::image::load_image;
use image::DynamicImage;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
};
use std::{
    io,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

const ASCII_RAMP: &[u8] = b" .,:;irsXA253hMHGS#9B&@";

pub(super) struct PreviewCache {
    pub(super) path: PathBuf,
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) mode: PreviewMode,
    pub(super) lines: Vec<Line<'static>>,
    pub(super) error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PreviewMode {
    Ascii,
    Color256,
}

impl PreviewMode {
    pub(super) fn toggle(self) -> Self {
        match self {
            Self::Ascii => Self::Color256,
            Self::Color256 => Self::Ascii,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Ascii => "ASCII",
            Self::Color256 => "COLOR",
        }
    }

    pub(super) fn description(self) -> &'static str {
        match self {
            Self::Ascii => "ASCII (default, fast, stable)",
            Self::Color256 => "256-color (higher fidelity, best effort)",
        }
    }
}

pub(super) fn render_preview(path: &Path, area: Rect, mode: PreviewMode) -> PreviewCache {
    let mut cache = PreviewCache {
        path: path.to_path_buf(),
        width: area.width,
        height: area.height,
        mode,
        lines: Vec::new(),
        error: None,
    };

    if area.width < 2 || area.height < 2 {
        cache.error = Some("Preview area is too small".into());
        return cache;
    }

    match load_preview_image(path, area.width, area.height) {
        Ok(image) => {
            cache.lines = image_to_lines(&image, area.width, area.height, mode);
        }
        Err(err) => {
            cache.error = Some(err);
        }
    }

    cache
}

fn load_preview_image(
    path: &Path,
    max_width: u16,
    max_height: u16,
) -> Result<DynamicImage, String> {
    let pixel_width = max_width.max(1) as u32;
    let pixel_height = (max_height.max(1) as u32).saturating_mul(2);

    match load_image_with_magick(path, pixel_width, pixel_height) {
        Ok(Some(image)) => Ok(image),
        Ok(None) => load_image(path),
        Err(magick_err) => load_image(path).map_err(|fallback_err| {
            format!("{magick_err}\nFallback decoder also failed: {fallback_err}")
        }),
    }
}

fn load_image_with_magick(
    path: &Path,
    pixel_width: u32,
    pixel_height: u32,
) -> Result<Option<DynamicImage>, String> {
    let output = match ProcessCommand::new("magick")
        .arg(path)
        .arg("-auto-orient")
        .arg("-thumbnail")
        .arg(format!("{pixel_width}x{pixel_height}>"))
        .arg("-colorspace")
        .arg("sRGB")
        .arg("png:-")
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(format!(
                "Failed to launch ImageMagick for {}: {}",
                path.display(),
                err
            ));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ImageMagick failed to render {}: {}",
            path.display(),
            stderr.trim()
        ));
    }

    image::load_from_memory_with_format(&output.stdout, image::ImageFormat::Png)
        .map(Some)
        .map_err(|e| {
            format!(
                "Failed to decode ImageMagick output for {}: {}",
                path.display(),
                e
            )
        })
}

fn image_to_lines(
    image: &image::DynamicImage,
    max_width: u16,
    max_height: u16,
    mode: PreviewMode,
) -> Vec<Line<'static>> {
    match mode {
        PreviewMode::Ascii => image_to_ascii_lines(image, max_width, max_height),
        PreviewMode::Color256 => image_to_color_lines(image, max_width, max_height),
    }
}

fn image_to_ascii_lines(
    image: &image::DynamicImage,
    max_width: u16,
    max_height: u16,
) -> Vec<Line<'static>> {
    let pixel_height = (max_height as u32).saturating_mul(2).max(1);
    let resized = image.thumbnail(max_width.max(1) as u32, pixel_height);
    let gray = resized.to_luma8();
    let (width, height) = gray.dimensions();

    let row_count = height.div_ceil(2) as usize;
    let horizontal_padding = max_width.saturating_sub(width as u16) as usize / 2;
    let vertical_padding = max_height.saturating_sub(row_count as u16) as usize / 2;
    let blank_line = " ".repeat(max_width as usize);

    let mut lines = Vec::with_capacity(max_height as usize);
    for _ in 0..vertical_padding {
        lines.push(Line::from(blank_line.clone()));
    }

    for y in (0..height).step_by(2) {
        let mut text = String::with_capacity(max_width as usize);
        if horizontal_padding > 0 {
            text.push_str(&" ".repeat(horizontal_padding));
        }

        for x in 0..width {
            let top = u16::from(gray.get_pixel(x, y)[0]);
            let bottom = if y + 1 < height {
                u16::from(gray.get_pixel(x, y + 1)[0])
            } else {
                top
            };
            let average = ((top + bottom) / 2) as u8;
            text.push(map_intensity_to_char(average));
        }

        let used_width = horizontal_padding + width as usize;
        if used_width < max_width as usize {
            text.push_str(&" ".repeat(max_width as usize - used_width));
        }
        lines.push(Line::from(text));
    }

    while lines.len() < max_height as usize {
        lines.push(Line::from(blank_line.clone()));
    }

    lines
}

fn image_to_color_lines(
    image: &image::DynamicImage,
    max_width: u16,
    max_height: u16,
) -> Vec<Line<'static>> {
    let pixel_height = (max_height as u32).saturating_mul(2).max(1);
    let resized = image.thumbnail(max_width.max(1) as u32, pixel_height);
    let rgba = resized.to_rgba8();
    let (width, height) = rgba.dimensions();

    let row_count = height.div_ceil(2) as usize;
    let horizontal_padding = max_width.saturating_sub(width as u16) as usize / 2;
    let vertical_padding = max_height.saturating_sub(row_count as u16) as usize / 2;
    let blank_line = " ".repeat(max_width as usize);

    let mut lines = Vec::with_capacity(max_height as usize);
    for _ in 0..vertical_padding {
        lines.push(Line::from(blank_line.clone()));
    }

    for y in (0..height).step_by(2) {
        let mut spans = Vec::with_capacity(width as usize + 2);
        if horizontal_padding > 0 {
            spans.push(Span::raw(" ".repeat(horizontal_padding)));
        }

        for x in 0..width {
            let top = rgba.get_pixel(x, y).0;
            let bottom = if y + 1 < height {
                rgba.get_pixel(x, y + 1).0
            } else {
                top
            };

            spans.push(Span::styled(
                "▀",
                Style::new()
                    .fg(rgb_to_256_color(top[0], top[1], top[2]))
                    .bg(rgb_to_256_color(bottom[0], bottom[1], bottom[2])),
            ));
        }

        let used_width = horizontal_padding + width as usize;
        if used_width < max_width as usize {
            spans.push(Span::raw(" ".repeat(max_width as usize - used_width)));
        }
        lines.push(Line::from(spans));
    }

    while lines.len() < max_height as usize {
        lines.push(Line::from(blank_line.clone()));
    }

    lines
}

fn map_intensity_to_char(intensity: u8) -> char {
    let index = (usize::from(intensity) * (ASCII_RAMP.len() - 1)) / 255;
    ASCII_RAMP[index] as char
}

fn rgb_to_256_color(r: u8, g: u8, b: u8) -> Color {
    let to_cube = |channel: u8| -> u8 { (((u16::from(channel) * 5) + 127) / 255) as u8 };

    let r = to_cube(r);
    let g = to_cube(g);
    let b = to_cube(b);
    Color::Indexed(16 + 36 * r + 6 * g + b)
}
