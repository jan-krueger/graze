use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, AppMode};
use super::popup::Popup;

pub struct ColumnPicker<'a> {
    app: &'a App,
}

impl<'a> ColumnPicker<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for ColumnPicker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let setup = &self.app.diff.setup;
        if setup.columns.is_empty() {
            return;
        }

        let title = match self.app.mode {
            AppMode::DiffSetupKey => "Select key column(s)",
            AppMode::DiffSetupCols => "Select columns to compare",
            AppMode::ColumnHide => "Show/hide columns",
            _ => return,
        };

        // Get column type info from schema
        let schema = self.app.tab().data.schema.as_ref();
        let type_strings: Vec<String> = setup
            .columns
            .iter()
            .map(|col_name| {
                schema
                    .and_then(|s| s.field_with_name(col_name).ok())
                    .map(|f| format!("{}", f.data_type()).to_lowercase())
                    .unwrap_or_default()
            })
            .collect();

        let max_name_len = setup.columns.iter().map(|c| c.width()).max().unwrap_or(8);
        let max_type_len = type_strings.iter().map(|t| t.width()).max().unwrap_or(4);
        let footer = "Space:toggle  Enter:confirm  Esc:cancel";

        // Content width: "  [x] name     type  "
        let content_width = 6 + max_name_len + 2 + max_type_len + 2;
        let max_visible = (area.height as usize).saturating_sub(6);
        let visible_rows = setup.columns.len().min(max_visible).max(1);

        let theme = &self.app.theme;
        let popup = Popup::new(title, footer, content_width, visible_rows, theme);
        let inner = popup.render_frame(area, buf);

        let normal_style = Style::default().fg(theme.fg);
        let selected_style = Style::default().fg(theme.fg).bg(theme.selected_bg);
        let dim_style = Style::default().fg(theme.dim);
        let check_style = Style::default().fg(Color::Green);

        let scroll_start = if setup.cursor >= visible_rows {
            setup.cursor - visible_rows + 1
        } else {
            0
        };

        for (i, col_idx) in (scroll_start..setup.columns.len())
            .take(visible_rows)
            .enumerate()
        {
            let row_y = inner.y + i as u16;
            if row_y >= inner.y + inner.height {
                break;
            }

            let is_cursor = col_idx == setup.cursor;
            let is_checked = setup.selected[col_idx];
            let row_style = if is_cursor { selected_style } else { normal_style };

            // Prefix
            let prefix = if is_cursor { ">> " } else { "   " };
            buf.set_string(inner.x, row_y, prefix, row_style);

            // Checkbox
            let checkbox = if is_checked { "[x]" } else { "[ ]" };
            let cb_style = if is_checked && !is_cursor { check_style } else { row_style };
            buf.set_string(inner.x + 3, row_y, checkbox, cb_style);

            // Column name
            let name = &setup.columns[col_idx];
            let padded_name = format!("{:<width$}", name, width = max_name_len);
            buf.set_string(inner.x + 7, row_y, &padded_name, row_style);

            // Type string
            let type_str = &type_strings[col_idx];
            let type_style = if is_cursor { row_style } else { dim_style };
            let type_x = inner.x + 7 + max_name_len as u16 + 2;
            buf.set_string(type_x, row_y, type_str, type_style);

            // Pad remainder
            let used = 7 + max_name_len + 2 + type_str.width();
            let pad = (inner.width as usize).saturating_sub(used);
            buf.set_string(
                type_x + type_str.width() as u16,
                row_y,
                &" ".repeat(pad),
                row_style,
            );
        }
    }
}
