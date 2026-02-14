use duckdb::arrow::array::Array;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::state::DiffMarker;
use crate::ui::table_render::{build_formatters, compute_column_widths, truncate_to_width, visible_columns};

pub struct DiffView<'a> {
    app: &'a App,
}

impl<'a> DiffView<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for DiffView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

        let diff = &self.app.diff;

        // Header bar (1 row)
        let header_y = area.y;
        let header_style = Style::default()
            .bg(Color::Cyan)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD);

        buf.set_string(
            area.x,
            header_y,
            " ".repeat(area.width as usize),
            header_style,
        );

        if diff.loading {
            buf.set_string(area.x + 1, header_y, "Computing diff...", header_style);
            return;
        }

        if let Some(ref error) = diff.error {
            let err_style = Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD);
            buf.set_string(
                area.x,
                header_y,
                " ".repeat(area.width as usize),
                err_style,
            );
            let msg = format!(" Error: {}", error);
            buf.set_string(area.x, header_y, &msg, err_style);
            return;
        }

        let schema = match diff.schema.as_ref() {
            Some(s) => s,
            None => return,
        };
        let batch = match diff.batch.as_ref() {
            Some(b) => b,
            None => return,
        };

        let total_rows = batch.num_rows();

        // Header text
        let header_text = format!(
            " Diff: {} vs {} | +{} -{} ~{} ={}",
            diff.file_a,
            diff.file_b,
            diff.counts.only_a,
            diff.counts.only_b,
            diff.counts.changed,
            diff.counts.common,
        );
        buf.set_string(area.x, header_y, &header_text, header_style);

        // Scroll position indicator
        if total_rows > 0 {
            let pos_text = format!(" {}/{} ", diff.scroll_offset + 1, total_rows);
            let pos_x = area.x + area.width - pos_text.width() as u16;
            buf.set_string(pos_x, header_y, &pos_text, header_style);
        }

        let fields = schema.fields();
        if fields.is_empty() || total_rows == 0 {
            return;
        }

        let formatters = build_formatters(batch);

        // Compute visible rows range
        let data_area_height = (area.height as usize).saturating_sub(2); // header + col headers
        let sample_end = (diff.scroll_offset + data_area_height).min(total_rows);

        let (headers, col_widths) = compute_column_widths(
            schema,
            batch,
            &formatters,
            &|_i, name, type_str| format!("{} [{}]", name, type_str),
            (diff.scroll_offset, sample_end),
            50,
        );

        let row_prefix_width = 3; // "+ " / "- " / "~ " / "  " + space
        let visible_cols = visible_columns(
            &col_widths,
            area.width as usize,
            diff.column_offset,
            row_prefix_width,
        );

        if visible_cols.is_empty() {
            return;
        }

        // Determine which columns are key columns vs diff columns
        let key_col_indices: Vec<usize> = fields
            .iter()
            .enumerate()
            .filter(|(_, f)| diff.key_columns.contains(f.name()))
            .map(|(i, _)| i)
            .collect();
        let diff_col_indices: Vec<usize> = fields
            .iter()
            .enumerate()
            .filter(|(_, f)| diff.diff_columns.contains(f.name()))
            .map(|(i, _)| i)
            .collect();

        // Render column headers
        let col_header_y = header_y + 1;
        let col_header_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let key_header_style = col_header_style.add_modifier(Modifier::UNDERLINED);

        buf.set_string(
            area.x,
            col_header_y,
            " ".repeat(area.width as usize),
            Style::default(),
        );
        buf.set_string(area.x, col_header_y, "   ", Style::default());

        let mut x = area.x + row_prefix_width as u16;
        for &col_idx in &visible_cols {
            let header = &headers[col_idx];
            let width = col_widths[col_idx] as usize;
            let is_key = key_col_indices.contains(&col_idx);
            let style = if is_key {
                key_header_style
            } else {
                col_header_style
            };

            let display = if header.width() > width {
                header.chars().take(width).collect::<String>()
            } else {
                format!("{:<width$}", header, width = width)
            };
            buf.set_string(x, col_header_y, &display, style);
            x += col_widths[col_idx] + 2;
        }

        // Render data rows
        let data_start_y = col_header_y + 1;

        for display_row in 0..data_area_height {
            let row_y = data_start_y + display_row as u16;
            if row_y >= area.y + area.height {
                break;
            }

            let data_row = diff.scroll_offset + display_row;

            if data_row >= total_rows {
                buf.set_string(
                    area.x,
                    row_y,
                    " ".repeat(area.width as usize),
                    Style::default(),
                );
                continue;
            }

            let marker = diff.markers[data_row];
            let (prefix, base_fg) = match marker {
                DiffMarker::OnlyA => ("+ ", Color::Green),
                DiffMarker::OnlyB => ("- ", Color::Red),
                DiffMarker::Changed => ("~ ", Color::White),
                DiffMarker::Common => ("  ", Color::DarkGray),
            };

            let base_style = Style::default().fg(base_fg);
            let dim_style = Style::default().fg(Color::DarkGray);

            // Clear line
            buf.set_string(
                area.x,
                row_y,
                " ".repeat(area.width as usize),
                Style::default(),
            );

            // Row prefix
            let prefix_style = match marker {
                DiffMarker::OnlyA => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                DiffMarker::OnlyB => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                DiffMarker::Changed => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                DiffMarker::Common => dim_style,
            };
            buf.set_string(area.x, row_y, " ", Style::default());
            buf.set_string(area.x + 1, row_y, prefix, prefix_style);

            let mut x = area.x + row_prefix_width as u16;
            for &col_idx in &visible_cols {
                let width = col_widths[col_idx] as usize;
                let column = batch.column(col_idx);
                let is_null = column.is_null(data_row);
                let is_key = key_col_indices.contains(&col_idx);
                let is_diff = diff_col_indices.contains(&col_idx);

                let (display_val, cell_style) = if is_null {
                    let style = if marker == DiffMarker::Common {
                        dim_style
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    ("NULL".to_string(), style)
                } else if let Some(ref fmt) = formatters[col_idx] {
                    let val = fmt.value(data_row).to_string();

                    let style = if marker == DiffMarker::Common {
                        dim_style
                    } else if is_key {
                        Style::default().fg(Color::Cyan)
                    } else if is_diff
                        && marker == DiffMarker::Changed
                        && data_row < diff.changed_cells.len()
                        && col_idx < diff.changed_cells[data_row].len()
                        && diff.changed_cells[data_row][col_idx]
                    {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else if !is_diff && !is_key {
                        // Non-diff, non-key column: dim
                        dim_style
                    } else {
                        base_style
                    };
                    (val, style)
                } else {
                    ("?".to_string(), base_style)
                };

                buf.set_string(x, row_y, &truncate_to_width(&display_val, width), cell_style);
                x += col_widths[col_idx] + 2;
            }
        }
    }
}
