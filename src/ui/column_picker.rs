use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, AppMode};

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

        // Compute dimensions
        let max_name_len = setup.columns.iter().map(|c| c.width()).max().unwrap_or(8);
        let max_type_len = type_strings.iter().map(|t| t.width()).max().unwrap_or(4);
        let footer = "Space:toggle  Enter:confirm  Esc:cancel";

        // Content width: "  [x] name     type  "
        let content_width = 6 + max_name_len + 2 + max_type_len + 2;
        let popup_width = content_width.max(title.width() + 4).max(footer.width() + 4);
        let popup_width = popup_width.min(area.width as usize - 2);

        let max_visible_rows = (area.height as usize).saturating_sub(6);
        let visible_rows = setup.columns.len().min(max_visible_rows).max(1);
        let popup_height = visible_rows + 4; // title + blank + rows + footer

        // Center the popup
        let popup_x = area.x + (area.width.saturating_sub(popup_width as u16 + 2)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_height as u16)) / 2;
        let inner_width = popup_width;

        let border_style = Style::default().fg(Color::Cyan);
        let title_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let normal_style = Style::default().fg(Color::White);
        let selected_style = Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray);
        let dim_style = Style::default().fg(Color::DarkGray);
        let check_style = Style::default().fg(Color::Green);

        // Scroll if needed
        let scroll_start = if setup.cursor >= visible_rows {
            setup.cursor - visible_rows + 1
        } else {
            0
        };

        // Top border
        let top_border = format!(
            "\u{250c}\u{2500} {} {}\u{2510}",
            title,
            "\u{2500}".repeat(inner_width.saturating_sub(title.width() + 4))
        );
        buf.set_string(popup_x, popup_y, &top_border, border_style);
        // Title is highlighted
        buf.set_string(popup_x + 3, popup_y, title, title_style);

        // Blank line
        let blank_line = format!(
            "\u{2502}{}\u{2502}",
            " ".repeat(inner_width)
        );
        buf.set_string(popup_x, popup_y + 1, &blank_line, border_style);

        // Column rows
        for (i, col_idx) in (scroll_start..setup.columns.len())
            .take(visible_rows)
            .enumerate()
        {
            let row_y = popup_y + 2 + i as u16;
            let is_cursor = col_idx == setup.cursor;
            let is_checked = setup.selected[col_idx];

            let row_style = if is_cursor {
                selected_style
            } else {
                normal_style
            };

            // Left border
            buf.set_string(popup_x, row_y, "\u{2502}", border_style);

            // Prefix: "  " or ">>"
            let prefix = if is_cursor { ">> " } else { "   " };
            buf.set_string(popup_x + 1, row_y, prefix, row_style);

            // Checkbox
            let checkbox = if is_checked { "[x]" } else { "[ ]" };
            let cb_style = if is_checked && !is_cursor {
                check_style
            } else {
                row_style
            };
            buf.set_string(popup_x + 4, row_y, checkbox, cb_style);

            // Column name
            let name = &setup.columns[col_idx];
            let padded_name = format!("{:<width$}", name, width = max_name_len);
            buf.set_string(popup_x + 8, row_y, &padded_name, row_style);

            // Type string (dim)
            let type_str = &type_strings[col_idx];
            let type_style = if is_cursor { row_style } else { dim_style };
            let type_x = popup_x + 8 + max_name_len as u16 + 2;
            buf.set_string(type_x, row_y, type_str, type_style);

            // Padding to right border
            let used = 8 + max_name_len + 2 + type_str.width();
            let pad = inner_width.saturating_sub(used);
            buf.set_string(
                type_x + type_str.width() as u16,
                row_y,
                " ".repeat(pad),
                row_style,
            );

            // Right border
            let right_x = popup_x + inner_width as u16 + 1;
            if right_x < area.x + area.width {
                buf.set_string(right_x, row_y, "\u{2502}", border_style);
            }
        }

        // Blank line before footer
        let footer_blank_y = popup_y + 2 + visible_rows as u16;
        buf.set_string(popup_x, footer_blank_y, &blank_line, border_style);

        // Bottom border with footer
        let bottom_y = footer_blank_y + 1;
        let footer_padded = format!(
            "\u{2514} {} {}\u{2518}",
            footer,
            "\u{2500}".repeat(inner_width.saturating_sub(footer.width() + 3))
        );
        buf.set_string(popup_x, bottom_y, &footer_padded, border_style);
        buf.set_string(popup_x + 2, bottom_y, footer, dim_style);
    }
}
