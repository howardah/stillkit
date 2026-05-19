use crate::preview;
use clap::{Arg, ArgAction, ArgMatches, Command};
use crossterm::event::{self, KeyCode};
use image::Rgba;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const FILLED_STAR: char = '★';
const EMPTY_STAR: char = '☆';
const MAX_RATING: u8 = 5;

pub fn subcommand() -> Command {
    Command::new("rate")
        .about("Rate images in a directory from 0-5 in a terminal UI")
        .arg(
            Arg::new("input")
                .help("Input directory to explore (defaults to current directory)")
                .index(1)
                .value_name("INPUT_DIR"),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help("Scan subdirectories recursively")
                .action(ArgAction::SetTrue),
        )
}

pub fn run(matches: &ArgMatches) {
    if let Err(err) = run_inner(matches) {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run_inner(matches: &ArgMatches) -> Result<(), String> {
    let root = matches
        .get_one::<String>("input")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("Cannot determine current directory"));
    let recursive = matches.get_flag("recursive");

    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }

    let entries = collect_entries(&root, recursive)?;
    if entries.is_empty() {
        return Err(format!("No image files found in {}", root.display()));
    }

    let mut app = App::new(root, entries);
    ratatui::run(|terminal| app.run(terminal))
        .map_err(|e| format!("failed to run TUI: {e}"))?;
    Ok(())
}

struct App {
    root: PathBuf,
    entries: Vec<ImageEntry>,
    list_state: ListState,
    preview: Option<PreviewCache>,
    status: Option<String>,
}

impl App {
    fn new(root: PathBuf, entries: Vec<ImageEntry>) -> Self {
        Self {
            root,
            entries,
            list_state: ListState::default().with_selected(Some(0)),
            preview: None,
            status: Some("Use arrows or j/k to move. Press 0-5 to rate. Press q to quit.".into()),
        }
    }

    fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(key) = event::read()?.as_key_press_event() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
                    KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
                    KeyCode::PageDown => self.move_selection(10),
                    KeyCode::PageUp => self.move_selection(-10),
                    KeyCode::Home => self.select_index(0),
                    KeyCode::End => self.select_index(self.entries.len().saturating_sub(1)),
                    KeyCode::Char(digit @ '0'..='5') => {
                        self.rate_selected(digit as u8 - b'0');
                    }
                    _ => {}
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let [content_area, footer_area] = frame.area().layout(
            &Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).spacing(1),
        );
        let [list_area, detail_area] = content_area.layout(
            &Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)]).spacing(1),
        );

        self.render_list(frame, list_area);
        self.render_detail(frame, detail_area);
        self.render_footer(frame, footer_area);
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|entry| ListItem::new(entry.display_path.as_str()))
            .collect();
        let list = List::new(items)
            .block(Block::bordered().title("Images"))
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
            .highlight_symbol(">> ")
            .scroll_padding(2);

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_detail(&mut self, frame: &mut Frame, area: Rect) {
        let [preview_area, info_area] =
            area.layout(&Layout::vertical([Constraint::Min(10), Constraint::Length(6)]).spacing(1));

        let Some((display_title, disk_name, stars)) = self.selected_entry().map(|entry| {
            (
                entry.display_title.clone(),
                entry.path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                entry.rating_label(),
            )
        }) else {
            frame.render_widget(
                Paragraph::new("No image selected").block(Block::bordered().title("Preview")),
                preview_area,
            );
            return;
        };

        let title = Line::from(display_title.clone()).centered();
        let block = Block::bordered()
            .title_top(title)
            .title_bottom(Line::from(stars).centered());
        let inner = block.inner(preview_area);
        frame.render_widget(block, preview_area);

        self.ensure_preview(inner);

        let preview = match &self.preview {
            Some(cache) if cache.error.is_none() => {
                Paragraph::new(cache.lines.clone()).block(Block::new())
            }
            Some(cache) => Paragraph::new(cache.error.clone().unwrap_or_default())
                .style(Style::new().yellow())
                .centered(),
            None => Paragraph::new(""),
        };
        frame.render_widget(preview, inner);

        let info_lines = vec![
            Line::from(vec![
                Span::from("Shown as: ").bold(),
                Span::from(display_title),
            ]),
            Line::from(vec![
                Span::from("On disk: ").bold(),
                Span::from(disk_name),
            ]),
            Line::from(vec![
                Span::from("Directory: ").bold(),
                Span::from(self.root.display().to_string()),
            ]),
            Line::from(vec![
                Span::from("Keys: ").bold(),
                Span::from("0-5 rate, q quit"),
            ]),
        ];

        frame.render_widget(
            Paragraph::new(Text::from(info_lines))
                .block(Block::bordered().title("Details"))
                .wrap(Wrap { trim: true }),
            info_area,
        );
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let footer = vec![
            Line::from("Navigate with arrows or j/k. Home/End jump. PageUp/PageDown move faster."),
            Line::from(self.status.clone().unwrap_or_else(|| {
                "Press 0-5 to apply or update the current rating.".to_string()
            })),
        ];

        frame.render_widget(
            Paragraph::new(Text::from(footer))
                .block(Block::bordered().title("Status"))
                .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn move_selection(&mut self, delta: isize) {
        let Some(current) = self.list_state.selected() else {
            self.select_index(0);
            return;
        };

        let max = self.entries.len().saturating_sub(1) as isize;
        let next = (current as isize + delta).clamp(0, max) as usize;
        self.select_index(next);
    }

    fn select_index(&mut self, index: usize) {
        self.list_state.select(Some(index));
        self.preview = None;
    }

    fn selected_entry(&self) -> Option<&ImageEntry> {
        self.list_state
            .selected()
            .and_then(|index| self.entries.get(index))
    }

    fn ensure_preview(&mut self, area: Rect) {
        let Some(path) = self.selected_entry().map(|entry| entry.path.clone()) else {
            self.preview = None;
            return;
        };

        if let Some(cache) = &self.preview {
            if cache.path == path && cache.width == area.width && cache.height == area.height {
                return;
            }
        }

        self.preview = Some(render_preview(&path, area));
    }

    fn rate_selected(&mut self, rating: u8) {
        let Some(index) = self.list_state.selected() else {
            return;
        };

        if rating > MAX_RATING {
            return;
        }

        let result = {
            let entry = &self.entries[index];
            rename_with_rating(entry, rating)
        };

        match result {
            Ok(new_path) => {
                let root = self.root.clone();
                let new_entry = ImageEntry::from_path(&root, new_path.clone());
                self.entries[index] = new_entry;
                self.preview = None;
                self.status = Some(format!(
                    "Updated rating to {} for {}",
                    rating_to_stars(rating),
                    new_path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(err) => {
                self.status = Some(err);
            }
        }
    }
}

#[derive(Clone)]
struct ImageEntry {
    path: PathBuf,
    display_path: String,
    display_title: String,
    original_stem: String,
    extension: Option<String>,
    rating: Option<u8>,
}

impl ImageEntry {
    fn from_path(root: &Path, path: PathBuf) -> Self {
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        let (stem, extension) = split_file_name(&file_name);
        let (original_stem, rating) = split_rating_suffix(&stem);
        let display_title = build_display_title(&original_stem, extension.as_deref());

        let relative = path.strip_prefix(root).unwrap_or(&path);
        let display_path = if relative.parent().is_none() || relative.parent() == Some(Path::new("")) {
            display_title.clone()
        } else {
            let mut relative_display = relative.to_path_buf();
            relative_display.set_file_name(&display_title);
            relative_display.to_string_lossy().into_owned()
        };

        Self {
            path,
            display_path,
            display_title,
            original_stem,
            extension,
            rating,
        }
    }

    fn rating_label(&self) -> String {
        self.rating
            .map(rating_to_stars)
            .unwrap_or_else(|| "unrated".to_string())
    }
}

struct PreviewCache {
    path: PathBuf,
    width: u16,
    height: u16,
    lines: Vec<Line<'static>>,
    error: Option<String>,
}

fn collect_entries(root: &Path, recursive: bool) -> Result<Vec<ImageEntry>, String> {
    let mut paths = Vec::new();

    if recursive {
        for entry in walkdir::WalkDir::new(root).into_iter().flatten() {
            let path = entry.path();
            if path.is_file() && preview::is_supported_image(path) {
                paths.push(path.to_path_buf());
            }
        }
    } else {
        let entries = fs::read_dir(root)
            .map_err(|e| format!("Failed to read directory {}: {}", root.display(), e))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && preview::is_supported_image(&path) {
                paths.push(path);
            }
        }
    }

    paths.sort_by(|a, b| {
        a.to_string_lossy()
            .to_lowercase()
            .cmp(&b.to_string_lossy().to_lowercase())
    });

    Ok(paths
        .into_iter()
        .map(|path| ImageEntry::from_path(root, path))
        .collect())
}

fn rename_with_rating(entry: &ImageEntry, rating: u8) -> Result<PathBuf, String> {
    let new_name = build_rated_file_name(&entry.original_stem, entry.extension.as_deref(), rating);
    let new_path = entry.path.with_file_name(new_name);

    if new_path == entry.path {
        return Ok(new_path);
    }

    if new_path.exists() {
        return Err(format!(
            "Cannot rename {} because {} already exists",
            entry.path.display(),
            new_path.display()
        ));
    }

    fs::rename(&entry.path, &new_path).map_err(|e| {
        format!(
            "Failed to rename {} to {}: {}",
            entry.path.display(),
            new_path.display(),
            e
        )
    })?;

    Ok(new_path)
}

fn render_preview(path: &Path, area: Rect) -> PreviewCache {
    let mut cache = PreviewCache {
        path: path.to_path_buf(),
        width: area.width,
        height: area.height,
        lines: Vec::new(),
        error: None,
    };

    if area.width < 2 || area.height < 2 {
        cache.error = Some("Preview area is too small".into());
        return cache;
    }

    match preview::load_image(path) {
        Ok(image) => {
            cache.lines = image_to_lines(&image, area.width, area.height);
        }
        Err(err) => {
            cache.error = Some(err);
        }
    }

    cache
}

fn image_to_lines(image: &image::DynamicImage, max_width: u16, max_height: u16) -> Vec<Line<'static>> {
    let pixel_height = (max_height as u32).saturating_mul(2).max(1);
    let resized = image.thumbnail(max_width.max(1) as u32, pixel_height);
    let rgba = resized.to_rgba8();
    let (width, height) = rgba.dimensions();

    let row_count = height.div_ceil(2) as usize;
    let horizontal_padding = max_width.saturating_sub(width as u16) as usize / 2;
    let vertical_padding = max_height.saturating_sub(row_count as u16) as usize / 2;

    let mut lines = Vec::with_capacity(max_height as usize);
    for _ in 0..vertical_padding {
        lines.push(Line::from(""));
    }

    for y in (0..height).step_by(2) {
        let mut spans = Vec::with_capacity(width as usize + usize::from(horizontal_padding > 0));
        if horizontal_padding > 0 {
            spans.push(Span::raw(" ".repeat(horizontal_padding)));
        }

        for x in 0..width {
            let top = rgba.get_pixel(x, y);
            let bottom = if y + 1 < height {
                rgba.get_pixel(x, y + 1)
            } else {
                &Rgba([0, 0, 0, 255])
            };

            spans.push(Span::styled(
                "▀",
                Style::new()
                    .fg(rgba_to_color(top))
                    .bg(rgba_to_color(bottom)),
            ));
        }

        lines.push(Line::from(spans));
    }

    while lines.len() < max_height as usize {
        lines.push(Line::from(""));
    }

    lines
}

fn rgba_to_color(pixel: &Rgba<u8>) -> Color {
    let [r, g, b, a] = pixel.0;
    if a == 255 {
        return Color::Rgb(r, g, b);
    }

    let alpha = u16::from(a);
    let blend = |channel: u8| -> u8 { ((u16::from(channel) * alpha) / 255) as u8 };
    Color::Rgb(blend(r), blend(g), blend(b))
}

fn split_file_name(file_name: &str) -> (String, Option<String>) {
    match file_name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_string(), Some(extension.to_string())),
        _ => (file_name.to_string(), None),
    }
}

fn split_rating_suffix(stem: &str) -> (String, Option<u8>) {
    let Some((base, suffix)) = stem.rsplit_once('_') else {
        return (stem.to_string(), None);
    };

    match parse_rating_suffix(suffix) {
        Some(rating) => (base.to_string(), Some(rating)),
        None => (stem.to_string(), None),
    }
}

fn parse_rating_suffix(suffix: &str) -> Option<u8> {
    if suffix.chars().count() != usize::from(MAX_RATING) {
        return None;
    }

    let mut filled = 0u8;
    for ch in suffix.chars() {
        match ch {
            FILLED_STAR => filled += 1,
            EMPTY_STAR => {}
            _ => return None,
        }
    }

    Some(filled)
}

fn build_display_title(stem: &str, extension: Option<&str>) -> String {
    match extension {
        Some(extension) => format!("{stem}.{extension}"),
        None => stem.to_string(),
    }
}

fn build_rated_file_name(stem: &str, extension: Option<&str>, rating: u8) -> String {
    let rated_stem = format!("{stem}_{}", rating_to_stars(rating));
    build_display_title(&rated_stem, extension)
}

fn rating_to_stars(rating: u8) -> String {
    let rating = rating.min(MAX_RATING);
    format!(
        "{}{}",
        FILLED_STAR.to_string().repeat(rating as usize),
        EMPTY_STAR
            .to_string()
            .repeat((MAX_RATING - rating) as usize)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rated_stems() {
        assert_eq!(split_rating_suffix("fish"), ("fish".to_string(), None));
        assert_eq!(split_rating_suffix("fish_★☆☆☆☆"), ("fish".to_string(), Some(1)));
        assert_eq!(split_rating_suffix("fish_☆☆☆☆☆"), ("fish".to_string(), Some(0)));
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
        assert_eq!(build_rated_file_name("fish", Some("jpg"), 1), "fish_★☆☆☆☆.jpg");
        assert_eq!(build_rated_file_name("fish", Some("jpg"), 5), "fish_★★★★★.jpg");
        assert_eq!(build_rated_file_name("fish", None, 0), "fish_☆☆☆☆☆");
    }
}
