use super::entry::{ImageEntry, MAX_RATING, rating_to_stars, rename_with_rating};
use super::preview::{PreviewCache, PreviewMode, render_preview};
use crossterm::event::{self, KeyCode};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{io, path::PathBuf};

pub(super) struct App {
    root: PathBuf,
    entries: Vec<ImageEntry>,
    list_state: ListState,
    preview_mode: PreviewMode,
    preview: Option<PreviewCache>,
    status: Option<String>,
}

impl App {
    pub(super) fn new(root: PathBuf, entries: Vec<ImageEntry>) -> Self {
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

    pub(super) fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
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

        if let Some(cache) = &self.preview
            && cache.path == path
            && cache.width == area.width
            && cache.height == area.height
            && cache.mode == self.preview_mode
        {
            return;
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

fn panel_block<'a>(title: &'a str) -> Block<'a> {
    Block::bordered()
        .title(title)
        .title_style(Style::new().add_modifier(Modifier::BOLD))
}
