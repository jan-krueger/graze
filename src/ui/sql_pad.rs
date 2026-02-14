use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::app::App;

/// Height of the SQL pad area: 1 header + 5 input lines = 6 lines
pub const SQL_PAD_HEIGHT: u16 = 6;

pub struct SqlPad<'a> {
    app: &'a App,
}

impl<'a> SqlPad<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for SqlPad<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width < 10 {
            return;
        }

        let header_y = area.y;
        let input_start_y = area.y + 1;
        let input_lines = (area.height - 1) as usize;

        // --- Header line ---
        let header_style = Style::default()
            .bg(Color::Magenta)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);

        buf.set_string(
            area.x,
            header_y,
            " ".repeat(area.width as usize),
            header_style,
        );

        let table_name = self
            .app
            .tab()
            .data
            .table_name
            .as_deref()
            .unwrap_or("unknown");

        let header_text = format!(" SQL | Table: {} ", table_name);
        buf.set_string(area.x, header_y, &header_text, header_style);

        if let Some(ref err) = self.app.sql.error {
            let err_style = Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD);
            let err_text = format!(" Err: {} ", err);
            let err_x = area.x + header_text.len().min(area.width as usize) as u16;
            let remaining = area.width.saturating_sub(err_x - area.x);
            if remaining > 0 {
                let truncated: String = err_text.chars().take(remaining as usize).collect();
                buf.set_string(err_x, header_y, &truncated, err_style);
            }
        }

        // --- Input lines ---
        let input_bg = Style::default().bg(Color::Black).fg(Color::White);
        let line_num_style = Style::default()
            .bg(Color::Black)
            .fg(Color::DarkGray);

        for i in 0..input_lines {
            let y = input_start_y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            buf.set_string(
                area.x,
                y,
                " ".repeat(area.width as usize),
                input_bg,
            );

            let line_num = format!("{:2}: ", i + 1);
            buf.set_string(area.x, y, &line_num, line_num_style);

            if i < self.app.sql.lines.len() {
                let line = &self.app.sql.lines[i];
                let max_chars = (area.width as usize).saturating_sub(4);
                let display: String = line.chars().take(max_chars).collect();
                buf.set_string(area.x + 4, y, &display, input_bg);
            }
        }
    }
}
