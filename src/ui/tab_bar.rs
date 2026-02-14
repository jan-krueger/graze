use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::app::App;

pub struct TabBar<'a> {
    app: &'a App,
}

impl<'a> TabBar<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for TabBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg_style = Style::default().bg(Color::Black).fg(Color::DarkGray);

        // Fill background
        buf.set_string(
            area.x,
            area.y,
            " ".repeat(area.width as usize),
            bg_style,
        );

        let active_style = Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);
        let inactive_style = Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White);

        let mut x = area.x;
        for (i, tab) in self.app.tabs.iter().enumerate() {
            let name = tab
                .data
                .file_name
                .as_deref()
                .unwrap_or("loading...");

            let label = format!(" {} ", name);
            let style = if i == self.app.active_tab {
                active_style
            } else {
                inactive_style
            };

            if x + label.len() as u16 > area.x + area.width {
                break;
            }

            buf.set_string(x, area.y, &label, style);
            x += label.len() as u16;

            // Separator
            if x < area.x + area.width {
                buf.set_string(x, area.y, " ", bg_style);
                x += 1;
            }
        }
    }
}
