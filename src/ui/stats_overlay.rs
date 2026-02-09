use duckdb::arrow::array::{Array, AsArray};
use duckdb::arrow::datatypes::DataType;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::ui::table::type_color;
use crate::ui::table_render::{build_formatters, compute_column_widths, truncate_to_width, visible_columns};

pub struct StatsOverlay<'a> {
    app: &'a App,
}

impl<'a> StatsOverlay<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for StatsOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

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

        let scroll_offset = self.app.stats.scroll_offset;
        let header_text = if let Some(ref b) = self.app.stats.batch {
            let total = b.num_rows();
            if total > 0 {
                format!(" Statistics | {}/{} | Esc ", scroll_offset + 1, total)
            } else {
                " Statistics | Esc ".to_string()
            }
        } else {
            " Statistics | Esc ".to_string()
        };
        buf.set_string(area.x, header_y, &header_text, header_style);

        if self.app.stats.loading {
            let loading_style = Style::default().fg(Color::Yellow);
            let msg = "Loading statistics...";
            let msg_y = header_y + 1;
            if msg_y < area.y + area.height {
                buf.set_string(
                    area.x,
                    msg_y,
                    " ".repeat(area.width as usize),
                    Style::default(),
                );
                buf.set_string(area.x + 1, msg_y, msg, loading_style);
            }
            return;
        }

        let schema = match self.app.stats.schema.as_ref() {
            Some(s) => s,
            None => return,
        };
        let batch = match self.app.stats.batch.as_ref() {
            Some(b) => b,
            None => return,
        };

        let batch_rows = batch.num_rows();
        let fields = schema.fields();

        if fields.is_empty() || batch_rows == 0 {
            return;
        }

        let formatters = build_formatters(batch);

        let column_name_idx = fields
            .iter()
            .position(|f| f.name() == "column_name");
        let column_type_idx = fields
            .iter()
            .position(|f| f.name() == "column_type");

        // Build Arrow type overrides and colors for each row
        let (arrow_type_overrides, row_type_colors): (Vec<Option<String>>, Vec<Option<Color>>) =
            if let (Some(cn_idx), Some(_ct_idx), Some(app_schema)) =
                (column_name_idx, column_type_idx, &self.app.data.schema)
            {
                let cn_col = batch.column(cn_idx);
                let cn_array_utf8 = match cn_col.data_type() {
                    DataType::Utf8 => Some(cn_col.as_string::<i32>()),
                    _ => None,
                };
                let cn_array_large = match cn_col.data_type() {
                    DataType::LargeUtf8 => Some(cn_col.as_string::<i64>()),
                    _ => None,
                };

                (0..batch_rows)
                    .map(|row| {
                        let name: Option<&str> = if let Some(arr) = cn_array_utf8 {
                            if arr.is_null(row) { None } else { Some(arr.value(row)) }
                        } else if let Some(arr) = cn_array_large {
                            if arr.is_null(row) { None } else { Some(arr.value(row)) }
                        } else {
                            None
                        };
                        let field = name.and_then(|n| app_schema.field_with_name(n).ok());
                        let type_str = field.map(|f| format!("{}", f.data_type()));
                        let color = field.map(|f| type_color(f.data_type()));
                        (type_str, color)
                    })
                    .unzip()
            } else {
                (vec![None; batch_rows], vec![None; batch_rows])
            };

        // data_rows_height = lines available for data (minus header bar and column header)
        let data_rows_height = (area.height as usize).saturating_sub(2);
        let sample_end = (scroll_offset + data_rows_height).min(batch_rows);

        // Compute column widths, accounting for type overrides in column_type column
        let (headers, col_widths) = compute_column_widths(
            schema,
            batch,
            &formatters,
            &|_i, name, _type_str| name.to_string(),
            (scroll_offset, sample_end),
            40,
        );

        // Override column_type widths to account for arrow type override values
        let mut col_widths = col_widths;
        if let Some(ct_idx) = column_type_idx {
            use unicode_width::UnicodeWidthStr;
            let mut max_w = col_widths[ct_idx];
            for row in scroll_offset..sample_end {
                if let Some(ref ov) = arrow_type_overrides[row] {
                    let w = ov.width() as u16;
                    if w > max_w {
                        max_w = w;
                    }
                }
            }
            col_widths[ct_idx] = max_w.clamp(4, 40);
        }

        let left_margin = 1; // 1 char left margin
        let visible_cols = visible_columns(&col_widths, area.width as usize, 0, left_margin);

        if visible_cols.is_empty() {
            return;
        }

        // Render column headers
        let col_header_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let col_header_y = header_y + 1;

        if col_header_y < area.y + area.height {
            buf.set_string(
                area.x,
                col_header_y,
                " ".repeat(area.width as usize),
                Style::default(),
            );

            let mut x = area.x + 1;
            for &col_idx in &visible_cols {
                let header = &headers[col_idx];
                let width = col_widths[col_idx] as usize;
                buf.set_string(
                    x,
                    col_header_y,
                    &truncate_to_width(header, width),
                    col_header_style,
                );
                x += col_widths[col_idx] + 2;
            }
        }

        // Render data rows
        let data_start_y = col_header_y + 1;
        let max_display_rows = (area.y + area.height).saturating_sub(data_start_y) as usize;

        let row_even_style = Style::default();
        let row_odd_style = Style::default().fg(Color::White);
        let null_style = Style::default().fg(Color::DarkGray);

        for display_row in 0..max_display_rows {
            let row_y = data_start_y + display_row as u16;
            if row_y >= area.y + area.height {
                break;
            }

            let batch_row = scroll_offset + display_row;
            if batch_row >= batch_rows {
                buf.set_string(
                    area.x,
                    row_y,
                    " ".repeat(area.width as usize),
                    Style::default(),
                );
                continue;
            }

            let row_style = if batch_row % 2 == 0 {
                row_even_style
            } else {
                row_odd_style
            };

            buf.set_string(
                area.x,
                row_y,
                " ".repeat(area.width as usize),
                row_style,
            );

            let mut x = area.x + 1;
            for &col_idx in &visible_cols {
                let width = col_widths[col_idx] as usize;
                let column = batch.column(col_idx);
                let is_null = column.is_null(batch_row);

                let (display_val, cell_style) = if is_null {
                    ("NULL".to_string(), null_style)
                } else if column_type_idx == Some(col_idx) {
                    if let Some(ref override_val) = arrow_type_overrides[batch_row] {
                        let style = match row_type_colors[batch_row] {
                            Some(c) => row_style.fg(c),
                            None => row_style,
                        };
                        (override_val.clone(), style)
                    } else if let Some(ref fmt) = formatters[col_idx] {
                        (fmt.value(batch_row).to_string(), row_style)
                    } else {
                        ("?".to_string(), row_style)
                    }
                } else if column_name_idx == Some(col_idx) {
                    if let Some(ref fmt) = formatters[col_idx] {
                        let val = fmt.value(batch_row).to_string();
                        (val, row_style.fg(Color::Cyan))
                    } else {
                        ("?".to_string(), row_style)
                    }
                } else if let Some(ref fmt) = formatters[col_idx] {
                    let val = fmt.value(batch_row).to_string();
                    (val, row_style)
                } else {
                    ("?".to_string(), row_style)
                };

                buf.set_string(x, row_y, &truncate_to_width(&display_val, width), cell_style);
                x += col_widths[col_idx] + 2;
            }
        }
    }
}
