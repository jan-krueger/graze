use duckdb::arrow::array::Array;
use duckdb::arrow::datatypes::DataType;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::event::SortState;
use crate::state::SelectionMode;
use crate::ui::table_render::{build_formatters, compute_column_widths, truncate_to_width, visible_columns};

/// Build the sort indicator suffix for a column header.
fn sort_indicator_string(state: &SortState, col_name: &str, multi: bool) -> String {
    if let Some(pos) = state.position(col_name) {
        let order = state.order_for(col_name).unwrap();
        let arrow = order.indicator();
        if multi {
            format!(" {}{}", arrow, pos + 1)
        } else {
            format!(" {}", arrow)
        }
    } else {
        String::new()
    }
}

/// Map an Arrow DataType to a display color by category.
pub fn type_color(data_type: &DataType) -> Color {
    match data_type {
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::Float16
        | DataType::Float32
        | DataType::Float64
        | DataType::Decimal128(_, _)
        | DataType::Decimal256(_, _) => Color::Green,

        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => Color::Yellow,

        DataType::Boolean => Color::Magenta,

        DataType::Date32
        | DataType::Date64
        | DataType::Timestamp(_, _)
        | DataType::Time32(_)
        | DataType::Time64(_)
        | DataType::Duration(_)
        | DataType::Interval(_) => Color::Blue,

        DataType::Binary | DataType::LargeBinary | DataType::FixedSizeBinary(_) => Color::Red,

        _ => Color::White,
    }
}

#[derive(Debug)]
struct SimpleFilter {
    column: String,
    op: String,
    value: String,
}

fn parse_simple_filter(filter: &str) -> Option<SimpleFilter> {
    let trimmed = filter.trim();

    let upper = trimmed.to_uppercase();
    if upper.contains(" AND ") || upper.contains(" OR ") || trimmed.contains('(') {
        return None;
    }

    let ops = &["!=", "<>", "<=", ">=", "=", "<", ">"];
    let kw_ops = &["ILIKE", "LIKE", "IS NOT", "IS"];

    for &kw in kw_ops {
        if let Some(pos) = upper.find(&format!(" {kw} ")) {
            let col_part = trimmed[..pos].trim();
            let val_part = trimmed[pos + kw.len() + 2..].trim();
            let column = unquote_column(col_part)?;
            let value = unquote_value(val_part);
            return Some(SimpleFilter {
                column,
                op: kw.to_string(),
                value,
            });
        }
    }

    for &op in ops {
        if let Some(pos) = trimmed.find(op) {
            let col_part = trimmed[..pos].trim();
            let val_part = trimmed[pos + op.len()..].trim();
            if col_part.is_empty() || val_part.is_empty() {
                continue;
            }
            let column = unquote_column(col_part)?;
            let value = unquote_value(val_part);
            return Some(SimpleFilter {
                column,
                op: op.to_string(),
                value,
            });
        }
    }

    None
}

fn unquote_column(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        Some(s[1..s.len() - 1].to_string())
    } else if s.contains(' ') {
        None
    } else {
        Some(s.to_string())
    }
}

fn unquote_value(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn cell_matches_filter(cell_text: &str, filter: &SimpleFilter) -> bool {
    match filter.op.as_str() {
        "=" => cell_text == filter.value,
        "!=" | "<>" => cell_text != filter.value,
        ">" | "<" | ">=" | "<=" => {
            if let (Ok(cell_num), Ok(filter_num)) = (
                cell_text.parse::<f64>(),
                filter.value.parse::<f64>(),
            ) {
                match filter.op.as_str() {
                    ">" => cell_num > filter_num,
                    "<" => cell_num < filter_num,
                    ">=" => cell_num >= filter_num,
                    "<=" => cell_num <= filter_num,
                    _ => false,
                }
            } else {
                match filter.op.as_str() {
                    ">" => cell_text > filter.value.as_str(),
                    "<" => cell_text < filter.value.as_str(),
                    ">=" => cell_text >= filter.value.as_str(),
                    "<=" => cell_text <= filter.value.as_str(),
                    _ => false,
                }
            }
        }
        "LIKE" => sql_like_match(cell_text, &filter.value, true),
        "ILIKE" => sql_like_match(cell_text, &filter.value, false),
        "IS" => {
            let upper_val = filter.value.to_uppercase();
            if upper_val == "NULL" {
                false
            } else {
                false
            }
        }
        "IS NOT" => {
            let upper_val = filter.value.to_uppercase();
            upper_val == "NULL"
        }
        _ => false,
    }
}

fn sql_like_match(text: &str, pattern: &str, case_sensitive: bool) -> bool {
    let text = if case_sensitive {
        text.to_string()
    } else {
        text.to_lowercase()
    };
    let pattern = if case_sensitive {
        pattern.to_string()
    } else {
        pattern.to_lowercase()
    };
    like_match(text.as_bytes(), pattern.as_bytes())
}

fn like_match(text: &[u8], pattern: &[u8]) -> bool {
    let mut t = 0;
    let mut p = 0;
    let mut star_t = usize::MAX;
    let mut star_p = usize::MAX;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'_' || pattern[p] == text[t]) {
            t += 1;
            p += 1;
        } else if p < pattern.len() && pattern[p] == b'%' {
            star_t = t;
            star_p = p;
            p += 1;
        } else if star_p != usize::MAX {
            star_t += 1;
            t = star_t;
            p = star_p + 1;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == b'%' {
        p += 1;
    }

    p == pattern.len()
}

pub struct TableView<'a> {
    app: &'a App,
}

impl<'a> TableView<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for TableView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let schema = match self.app.data.schema.as_ref() {
            Some(s) => s,
            None => return,
        };
        let batch = match self.app.data.current_batch.as_ref() {
            Some(b) => b,
            None => return,
        };

        if area.height < 2 || area.width < 4 {
            return;
        }

        let batch_rows = batch.num_rows();
        let fields = schema.fields();

        let view_off = self.app.view_offset_in_buffer();
        let visible_count = self.app.visible_row_count();

        let formatters = build_formatters(batch);

        let multi = self.app.data.sort_state.specs().len() > 1;
        let sort_state = &self.app.data.sort_state;

        let (headers, col_widths) = compute_column_widths(
            schema,
            batch,
            &formatters,
            &|_i, name, type_str| {
                let sort_ind = sort_indicator_string(sort_state, name, multi);
                format!("{} [{}]{}", name, type_str, sort_ind)
            },
            (view_off, (view_off + visible_count).min(batch_rows)),
            50,
        );

        let row_prefix_width = 3;
        let visible_cols = visible_columns(
            &col_widths,
            area.width as usize,
            self.app.viewport.column_offset,
            row_prefix_width,
        );

        if visible_cols.is_empty() {
            return;
        }

        let col_select_active = self.app.viewport.selection_mode == SelectionMode::Column;
        let selected_col = self.app.viewport.selected_col;

        let header_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let mut x = area.x + row_prefix_width as u16;
        let header_y = area.y;

        buf.set_string(
            area.x,
            header_y,
            " ".repeat(area.width as usize),
            Style::default(),
        );

        for &col_idx in &visible_cols {
            let field = &fields[col_idx];
            let header = &headers[col_idx];
            let width = col_widths[col_idx] as usize;
            let is_col_selected = col_select_active && col_idx == selected_col;

            let col_header_style = if is_col_selected {
                header_style.bg(Color::DarkGray)
            } else {
                header_style
            };

            if header.width() > width {
                let display: String = header.chars().take(width).collect();
                buf.set_string(x, header_y, &display, col_header_style);
            } else {
                let name_part = field.name();
                buf.set_string(x, header_y, name_part, col_header_style);
                let mut cx = x + name_part.width() as u16;

                let type_str = format!("{}", field.data_type()).to_lowercase();
                let tc = type_color(field.data_type());
                let mut type_style = Style::default().fg(tc).add_modifier(Modifier::BOLD);
                if is_col_selected {
                    type_style = type_style.bg(Color::DarkGray);
                }

                let bracket_style = if is_col_selected {
                    col_header_style
                } else {
                    header_style
                };
                buf.set_string(cx, header_y, " [", bracket_style);
                cx += 2;
                buf.set_string(cx, header_y, &type_str, type_style);
                cx += type_str.width() as u16;
                buf.set_string(cx, header_y, "]", bracket_style);
                cx += 1;

                let sort_indicator =
                    sort_indicator_string(&self.app.data.sort_state, field.name(), multi);
                if !sort_indicator.is_empty() {
                    buf.set_string(cx, header_y, &sort_indicator, col_header_style);
                    cx += sort_indicator.width() as u16;
                }

                let used = (cx - x) as usize;
                if used < width {
                    let pad_style = if is_col_selected {
                        Style::default().bg(Color::DarkGray)
                    } else {
                        Style::default()
                    };
                    buf.set_string(
                        cx,
                        header_y,
                        " ".repeat(width - used),
                        pad_style,
                    );
                }
            }
            x += col_widths[col_idx] + 2;
        }

        let simple_filter = self
            .app
            .filter
            .active_filter
            .as_ref()
            .and_then(|f| parse_simple_filter(f));

        let highlight_col_idx: Option<usize> = simple_filter.as_ref().and_then(|sf| {
            fields
                .iter()
                .position(|f| f.name().eq_ignore_ascii_case(&sf.column))
        });

        let search_term: Option<String> = self
            .app
            .search
            .active_search
            .as_ref()
            .map(|s| s.to_lowercase());

        let data_start_y = header_y + 1;
        let max_display_rows = (area.height - 1) as usize;

        for display_row in 0..max_display_rows {
            let row_y = data_start_y + display_row as u16;
            if row_y >= area.y + area.height {
                break;
            }

            let batch_row = view_off + display_row;

            if display_row >= visible_count || batch_row >= batch_rows {
                buf.set_string(
                    area.x,
                    row_y,
                    " ".repeat(area.width as usize),
                    Style::default(),
                );
                continue;
            }

            let is_selected_row =
                display_row == self.app.viewport.selected_row_in_view();
            let show_row_highlight = is_selected_row && !col_select_active;
            let row_style = if show_row_highlight {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            buf.set_string(
                area.x,
                row_y,
                " ".repeat(area.width as usize),
                row_style,
            );

            let prefix = if show_row_highlight { ">> " } else { "   " };
            buf.set_string(
                area.x,
                row_y,
                prefix,
                if show_row_highlight {
                    row_style.fg(Color::Yellow)
                } else {
                    row_style
                },
            );

            let mut x = area.x + row_prefix_width as u16;
            for &col_idx in &visible_cols {
                let width = col_widths[col_idx] as usize;
                let column = batch.column(col_idx);
                let is_null = column.is_null(batch_row);
                let is_col_selected = col_select_active && col_idx == selected_col;
                let base_style = if is_col_selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    row_style
                };

                let (display_val, cell_style) = if is_null {
                    ("NULL".to_string(), base_style.fg(Color::DarkGray))
                } else if let Some(ref fmt) = formatters[col_idx] {
                    let val = fmt.value(batch_row).to_string();
                    let mut style = if is_col_selected {
                        base_style
                    } else if highlight_col_idx == Some(col_idx) {
                        if let Some(ref sf) = simple_filter {
                            if cell_matches_filter(&val, sf) {
                                base_style.fg(Color::Yellow)
                            } else {
                                base_style
                            }
                        } else {
                            base_style
                        }
                    } else {
                        base_style
                    };
                    if let Some(ref term) = search_term {
                        if val.to_lowercase().contains(term.as_str()) {
                            style = style.fg(Color::Yellow).bg(Color::Black);
                        }
                    }
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
