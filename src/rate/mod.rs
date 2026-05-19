use crate::preview;
use clap::{Arg, ArgAction, ArgMatches, Command};
use crossterm::event::{self, KeyCode};
use image::DynamicImage;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

const FILLED_STAR: char = '★';
const EMPTY_STAR: char = '☆';
const MAX_RATING: u8 = 5;
const ASCII_RAMP: &[u8] = b" .,:;irsXA253hMHGS#9B&@";

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
        .subcommand(import_subcommand())
}

pub fn run(matches: &ArgMatches) {
    let result = match matches.subcommand() {
        Some(("import", sub_matches)) => run_import(sub_matches),
        _ => run_inner(matches),
    };

    if let Err(err) = result {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn import_subcommand() -> Command {
    Command::new("import")
        .about("Import ratings from one file or directory into another")
        .arg(
            Arg::new("from_flag")
                .long("from")
                .value_name("FROM")
                .help("Source file or directory that already contains ratings")
                .conflicts_with("from"),
        )
        .arg(
            Arg::new("to_flag")
                .long("to")
                .value_name("TO")
                .help("Target file or directory to receive ratings")
                .conflicts_with("to"),
        )
        .arg(
            Arg::new("from")
                .index(1)
                .value_name("FROM")
                .help("Source file or directory that already contains ratings"),
        )
        .arg(
            Arg::new("to")
                .index(2)
                .value_name("TO")
                .help("Target file or directory to receive ratings"),
        )
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
    ratatui::run(|terminal| app.run(terminal)).map_err(|e| format!("failed to run TUI: {e}"))?;
    Ok(())
}

struct App {
    root: PathBuf,
    entries: Vec<ImageEntry>,
    list_state: ListState,
    preview_mode: PreviewMode,
    preview: Option<PreviewCache>,
    status: Option<String>,
}

impl App {
    fn new(root: PathBuf, entries: Vec<ImageEntry>) -> Self {
        Self {
            root,
            entries,
            list_state: ListState::default().with_selected(Some(0)),
            preview_mode: PreviewMode::Color256,
            preview: None,
            status: Some(
                "Use arrows or j/k to move. Press 0-5 to rate. Press p to toggle preview mode. Press q to quit."
                    .into(),
            ),
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
                    KeyCode::Char('p') => self.toggle_preview_mode(),
                    _ => {}
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let [content_area, footer_area] = frame
            .area()
            .layout(&Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).spacing(1));
        let [list_area, detail_area] = content_area.layout(
            &Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)])
                .spacing(1),
        );

        self.render_list(frame, list_area);
        self.render_detail(frame, detail_area);
        self.render_footer(frame, footer_area);
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect) {
        let block = panel_block("Images");
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|entry| {
                ListItem::new(Line::from(vec![
                    Span::raw(entry.display_path.clone()),
                    Span::styled(
                        entry
                            .rating
                            .map(|rating| format!("  {}", rating_to_stars(rating)))
                            .unwrap_or_default(),
                        Style::new().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();
        let list = List::new(items)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
            .highlight_symbol(">> ")
            .scroll_padding(2);

        frame.render_widget(Clear, inner);
        frame.render_stateful_widget(list, inner, &mut self.list_state);
    }

    fn render_detail(&mut self, frame: &mut Frame, area: Rect) {
        let [preview_area, info_area] =
            area.layout(&Layout::vertical([Constraint::Min(10), Constraint::Length(6)]).spacing(1));

        let Some((display_title, disk_name, stars)) = self.selected_entry().map(|entry| {
            (
                entry.display_title.clone(),
                entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
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
        let block = panel_block("").title_top(title).title_bottom(
            Line::from(format!("{stars}  [{}]", self.preview_mode.label())).centered(),
        );
        let inner = block.inner(preview_area);
        frame.render_widget(Clear, preview_area);
        frame.render_widget(block, preview_area);

        self.ensure_preview(inner);

        let preview = match &self.preview {
            Some(cache) if cache.error.is_none() => {
                Paragraph::new(cache.lines.clone()).block(Block::new())
            }
            Some(cache) => Paragraph::new(cache.error.clone().unwrap_or_default())
                .style(Style::new().fg(Color::Yellow))
                .centered(),
            None => Paragraph::new(""),
        };
        frame.render_widget(Clear, inner);
        frame.render_widget(preview, inner);

        let info_lines = vec![
            Line::from(vec![
                Span::styled("Shown as: ", Style::new().add_modifier(Modifier::BOLD)),
                Span::raw(display_title),
            ]),
            Line::from(vec![
                Span::styled("On disk: ", Style::new().add_modifier(Modifier::BOLD)),
                Span::raw(disk_name),
            ]),
            Line::from(vec![
                Span::styled("Directory: ", Style::new().add_modifier(Modifier::BOLD)),
                Span::styled(
                    self.root.display().to_string(),
                    Style::new().fg(Color::DarkGray),
                ),
            ]),
            Line::from(vec![
                Span::styled("Keys: ", Style::new().add_modifier(Modifier::BOLD)),
                Span::styled(
                    "0-5 rate, p toggle preview, q quit",
                    Style::new().fg(Color::DarkGray),
                ),
            ]),
        ];

        frame.render_widget(Clear, info_area);
        frame.render_widget(
            Paragraph::new(Text::from(info_lines))
                .block(panel_block("Details"))
                .wrap(Wrap { trim: true }),
            info_area,
        );
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let footer = vec![
            Line::from(Span::styled(
                "Navigate with arrows or j/k. Home/End jump. PageUp/PageDown move faster.",
                Style::new().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                self.status.clone().unwrap_or_else(|| {
                    "Press 0-5 to apply or update the current rating.".to_string()
                }),
                Style::new(),
            )),
        ];

        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(Text::from(footer))
                .block(panel_block("Status"))
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
            if cache.path == path
                && cache.width == area.width
                && cache.height == area.height
                && cache.mode == self.preview_mode
            {
                return;
            }
        }

        self.preview = Some(render_preview(&path, area, self.preview_mode));
    }

    fn toggle_preview_mode(&mut self) {
        self.preview_mode = self.preview_mode.toggle();
        self.preview = None;
        self.status = Some(format!("Preview mode: {}", self.preview_mode.description()));
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
        let display_path =
            if relative.parent().is_none() || relative.parent() == Some(Path::new("")) {
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
    mode: PreviewMode,
    lines: Vec<Line<'static>>,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewMode {
    Ascii,
    Color256,
}

impl PreviewMode {
    fn toggle(self) -> Self {
        match self {
            Self::Ascii => Self::Color256,
            Self::Color256 => Self::Ascii,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Ascii => "ASCII",
            Self::Color256 => "COLOR",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Ascii => "ASCII (default, fast, stable)",
            Self::Color256 => "256-color (higher fidelity, best effort)",
        }
    }
}

#[derive(Default)]
struct ImportStats {
    updated: usize,
    unchanged: usize,
    missing_source: usize,
    source_unrated: usize,
    failed: usize,
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

fn run_import(matches: &ArgMatches) -> Result<(), String> {
    let from = matches
        .get_one::<String>("from_flag")
        .or_else(|| matches.get_one::<String>("from"))
        .map(PathBuf::from)
        .ok_or_else(|| "missing --from/ FROM argument".to_string())?;
    let to = matches
        .get_one::<String>("to_flag")
        .or_else(|| matches.get_one::<String>("to"))
        .map(PathBuf::from)
        .ok_or_else(|| "missing --to/ TO argument".to_string())?;

    let source_files = collect_import_paths(&from)?;
    if source_files.is_empty() {
        return Err(format!("No image files found in {}", from.display()));
    }

    let target_files = collect_import_paths(&to)?;
    if target_files.is_empty() {
        return Err(format!("No image files found in {}", to.display()));
    }

    let (rating_index, unrated_source_count) = build_rating_index(&source_files);
    let mut stats = ImportStats::default();

    for target_path in target_files {
        let entry = image_entry_for_path(target_path.clone());
        let key = normalized_name_key(&entry.original_stem);

        let Some(source_rating) = rating_index.get(&key).copied() else {
            stats.missing_source += 1;
            continue;
        };

        if entry.rating == Some(source_rating) {
            stats.unchanged += 1;
            continue;
        }

        match rename_with_rating(&entry, source_rating) {
            Ok(_) => stats.updated += 1,
            Err(err) => {
                stats.failed += 1;
                eprintln!("{err}");
            }
        }
    }

    stats.source_unrated = unrated_source_count;

    println!(
        "Imported ratings: {} updated, {} unchanged, {} without source match, {} unrated sources ignored, {} failed",
        stats.updated, stats.unchanged, stats.missing_source, stats.source_unrated, stats.failed
    );

    Ok(())
}

fn collect_import_paths(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut result = Vec::new();

    if path.is_file() {
        if preview::is_supported_image(path) {
            result.push(path.to_path_buf());
        }
        return Ok(result);
    }

    if !path.is_dir() {
        return Err(format!("{} is not a file or directory", path.display()));
    }

    let entries = fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory {}: {}", path.display(), e))?;
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_file() && preview::is_supported_image(&child) {
            result.push(child);
        }
    }

    result.sort_by(|a, b| {
        a.to_string_lossy()
            .to_lowercase()
            .cmp(&b.to_string_lossy().to_lowercase())
    });

    Ok(result)
}

fn build_rating_index(paths: &[PathBuf]) -> (HashMap<String, u8>, usize) {
    let mut index = HashMap::new();
    let mut unrated = 0;

    for path in paths {
        let entry = image_entry_for_path(path.clone());
        let Some(rating) = entry.rating else {
            unrated += 1;
            continue;
        };

        index
            .entry(normalized_name_key(&entry.original_stem))
            .or_insert(rating);
    }

    (index, unrated)
}

fn image_entry_for_path(path: PathBuf) -> ImageEntry {
    let root = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    ImageEntry::from_path(&root, path)
}

fn normalized_name_key(name: &str) -> String {
    name.to_lowercase()
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

fn render_preview(path: &Path, area: Rect, mode: PreviewMode) -> PreviewCache {
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
        Ok(None) => preview::load_image(path),
        Err(magick_err) => preview::load_image(path).map_err(|fallback_err| {
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

fn split_file_name(file_name: &str) -> (String, Option<String>) {
    match file_name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => {
            (stem.to_string(), Some(extension.to_string()))
        }
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

fn panel_block<'a>(title: &'a str) -> Block<'a> {
    Block::bordered()
        .title(title)
        .title_style(Style::new().add_modifier(Modifier::BOLD))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

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
        std::env::temp_dir().join(format!("photool-rate-{label}-{}-{nanos}", process::id()))
    }
}
