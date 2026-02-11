use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::app::{App, AppMode};
use crate::state::{SearchMode, SelectionMode};

pub struct StatusBar<'a> {
    app: &'a App,
}

impl<'a> StatusBar<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg_style = Style::default().bg(Color::DarkGray).fg(Color::White);

        buf.set_string(
            area.x,
            area.y,
            " ".repeat(area.width as usize),
            bg_style,
        );

        // Mode indicator
        let mode_str = self.app.mode.badge();
        let mode_style = self.app.mode.badge_style();
        buf.set_string(area.x, area.y, mode_str, mode_style);

        let mut x = area.x + mode_str.len() as u16;

        // Selection mode indicator (only in Normal mode)
        if self.app.mode == AppMode::Normal {
            let sel_str = match self.app.viewport.selection_mode {
                SelectionMode::Row => " ROW ",
                SelectionMode::Column => " COL ",
            };
            let sel_style = match self.app.viewport.selection_mode {
                SelectionMode::Row => Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                SelectionMode::Column => Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            };
            buf.set_string(x, area.y, sel_str, sel_style);
            x += sel_str.len() as u16;
        }

        // File name
        if let Some(ref name) = self.app.data.file_name {
            let file_str = format!(" {} ", name);
            buf.set_string(x, area.y, &file_str, bg_style);
            x += file_str.len() as u16;
        }

        // Position info
        if self.app.data.total_rows > 0 {
            let pos = self.app.absolute_row() + 1;
            let total = self.app.data.total_rows;
            let pos_str = format!(" {pos}/{total} ");
            buf.set_string(x, area.y, &pos_str, bg_style);
            x += pos_str.len() as u16;
        }

        // Active filter indicator
        if let Some(ref filter) = self.app.filter.active_filter {
            let filter_str = format!(" [Filter: {}] ", filter);
            let filter_style = bg_style.fg(Color::Green);
            buf.set_string(x, area.y, &filter_str, filter_style);
            x += filter_str.len() as u16;
        }

        // Active search indicator
        if let Some(ref search) = self.app.search.active_search {
            let (label, color) = match self.app.search.search_mode {
                SearchMode::Regex => ("Regex", Color::Magenta),
                SearchMode::Plain => ("Search", Color::Yellow),
            };
            let search_str = match (self.app.search.match_index, self.app.search.match_count) {
                (Some(idx), Some(total)) => {
                    format!(" [{}: {} ({}/{} matches)] ", label, search, idx, total)
                }
                (None, Some(total)) => {
                    format!(" [{}: {} ({} matches)] ", label, search, total)
                }
                _ => format!(" [{}: {}] ", label, search),
            };
            let search_style = bg_style.fg(color);
            buf.set_string(x, area.y, &search_str, search_style);
            x += search_str.len() as u16;
        }

        // Status message
        if let Some(ref msg) = self.app.status_message {
            let msg_str = format!(" | {msg} ");
            let msg_style = bg_style.fg(Color::Yellow);
            buf.set_string(x, area.y, &msg_str, msg_style);
            x += msg_str.len() as u16;
        }

        // Right-aligned help hints
        let hints = self.app.mode.hints_string();
        let hints_width = hints.len() as u16;
        if area.width > hints_width + x - area.x + 1 {
            let hints_x = area.x + area.width - hints_width - 1;
            buf.set_string(
                hints_x,
                area.y,
                &hints,
                bg_style.add_modifier(Modifier::DIM),
            );
        }
    }
}
