use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

use crate::app::App;
use super::popup::Popup;

const MAX_VISIBLE_ROWS: usize = 15;

pub struct ColumnJump<'a> {
    app: &'a App,
}

impl<'a> ColumnJump<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for ColumnJump<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let cj = &self.app.column_jump;
        let theme = &self.app.theme;

        // Content: 1 line for query + 1 blank + up to MAX_VISIBLE_ROWS of results
        let result_count = cj.filtered.len().min(MAX_VISIBLE_ROWS);
        let content_height = 1 + 1 + result_count; // query line + separator + results

        let max_name_len = cj.all_columns.iter().map(|(_, n)| n.len()).max().unwrap_or(10);
        let idx_width = format!("{}", cj.all_columns.len()).len();
        let content_width = (idx_width + 3 + max_name_len + 4).max(30);

        let title = "Jump to Column";
        let footer = "Type to filter, Enter to jump";
        let popup = Popup::new(title, footer, content_width, content_height, theme);
        let inner = popup.render_frame(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let query_style = Style::default().fg(theme.fg).add_modifier(Modifier::BOLD);
        let cursor_style = Style::default().fg(theme.bg).bg(theme.fg);
        let dim_style = Style::default().fg(theme.dim);
        let normal_style = Style::default().fg(theme.fg);
        let selected_style = Style::default().fg(theme.fg).bg(theme.selected_bg);

        // Query line with cursor
        let query_display = format!(" > {}", cj.query);
        let padded: String = format!("{:<width$}", query_display, width = inner.width as usize);
        buf.set_string(inner.x, inner.y, &padded, query_style);
        // Draw cursor block
        let cursor_x = inner.x + 3 + cj.query.len() as u16;
        if cursor_x < inner.x + inner.width {
            buf.set_string(cursor_x, inner.y, " ", cursor_style);
        }

        // Separator line
        if inner.height > 1 {
            let sep: String = "\u{2500}".repeat(inner.width as usize);
            buf.set_string(inner.x, inner.y + 1, &sep, dim_style);
        }

        if cj.filtered.is_empty() {
            if inner.height > 2 {
                let msg = " No matching columns";
                let padded = format!("{:<width$}", msg, width = inner.width as usize);
                buf.set_string(inner.x, inner.y + 2, &padded, dim_style);
            }
            return;
        }

        // Scroll so cursor is visible
        let visible_rows = result_count;
        let scroll_start = if cj.cursor >= visible_rows {
            cj.cursor - visible_rows + 1
        } else {
            0
        };

        for (i, &all_idx) in cj.filtered.iter().skip(scroll_start).take(visible_rows).enumerate() {
            let y = inner.y + 2 + i as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let (col_idx, ref name) = cj.all_columns[all_idx];
            let is_selected = scroll_start + i == cj.cursor;
            let style = if is_selected { selected_style } else { normal_style };

            let idx_str = format!("{:>width$}", col_idx, width = idx_width);
            let line = format!(" {} {}", idx_str, name);
            let padded = format!("{:<width$}", line, width = inner.width as usize);
            buf.set_string(inner.x, y, &padded, style);

            // Dim the index portion
            if !is_selected {
                buf.set_string(inner.x + 1, y, &idx_str, dim_style);
            }
        }
    }
}
