use duckdb::arrow::array::{Array, AsArray};
use duckdb::arrow::datatypes::DataType;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::ui::table_render::{type_color, TableStyler, UnifiedTable};

pub struct StatsOverlay<'a> {
    app: &'a App,
}

impl<'a> StatsOverlay<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

/// Styler for stats: alternating row colors, column_name in cyan, column_type with arrow type overrides.
struct StatsStyler {
    column_name_idx: Option<usize>,
    column_type_idx: Option<usize>,
    arrow_type_overrides: Vec<Option<String>>,
    row_type_colors: Vec<Option<Color>>,
    scroll_offset: usize,
}

impl TableStyler for StatsStyler {
    fn row_prefix(&self, _data_row: usize) -> (&str, Style) {
        (" ", Style::default())
    }

    fn row_bg(&self, data_row: usize) -> Style {
        let abs_row = self.scroll_offset + data_row;
        if abs_row % 2 == 0 {
            Style::default()
        } else {
            Style::default().fg(Color::White)
        }
    }

    fn cell(
        &self,
        col_idx: usize,
        data_row: usize,
        formatted: &str,
        is_null: bool,
        base: Style,
    ) -> (String, Style) {
        if is_null {
            return ("NULL".to_string(), Style::default().fg(Color::DarkGray));
        }

        let abs_row = self.scroll_offset + data_row;

        if self.column_type_idx == Some(col_idx) {
            if let Some(ref override_val) = self.arrow_type_overrides.get(abs_row).and_then(|v| v.as_ref()) {
                let style = match self.row_type_colors.get(abs_row).and_then(|c| *c) {
                    Some(c) => base.fg(c),
                    None => base,
                };
                return (override_val.to_string(), style);
            }
        }

        if self.column_name_idx == Some(col_idx) {
            return (formatted.to_string(), base.fg(Color::Cyan));
        }

        (formatted.to_string(), base)
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

        let column_name_idx = fields
            .iter()
            .position(|f| f.name() == "column_name");
        let column_type_idx = fields
            .iter()
            .position(|f| f.name() == "column_type");

        // Build Arrow type overrides and colors for each row
        let (arrow_type_overrides, row_type_colors): (Vec<Option<String>>, Vec<Option<Color>>) =
            if let (Some(cn_idx), Some(_ct_idx), Some(app_schema)) =
                (column_name_idx, column_type_idx, &self.app.tab().data.schema)
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

        // Compute column_type width adjustment for arrow type overrides
        let col_width_adj: Vec<(usize, u16)> = if let Some(ct_idx) = column_type_idx {
            let data_rows_height = (area.height as usize).saturating_sub(2);
            let sample_end = (scroll_offset + data_rows_height).min(batch_rows);
            let mut max_w: u16 = 0;
            for row in scroll_offset..sample_end {
                if let Some(ref ov) = arrow_type_overrides[row] {
                    let w = ov.width() as u16;
                    if w > max_w {
                        max_w = w;
                    }
                }
            }
            if max_w > 0 {
                vec![(ct_idx, max_w)]
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        let styler = StatsStyler {
            column_name_idx,
            column_type_idx,
            arrow_type_overrides,
            row_type_colors,
            scroll_offset,
        };

        // Table area starts below the cyan header bar
        let table_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(1),
        };

        UnifiedTable::new(schema, batch, &styler)
            .scroll_offset(scroll_offset)
            .left_margin(1)
            .max_col_width(40)
            .show_types(false)
            .col_width_mins(&col_width_adj)
            .render(table_area, buf);
    }
}
