use std::sync::Arc;

use duckdb::arrow::array::Array;
use duckdb::arrow::datatypes::{DataType, Schema};
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::arrow::util::display::ArrayFormatter;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::event::SortState;

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

/// Convert a 1-based position to a subscript digit character.
pub fn subscript_digit(n: usize) -> char {
    const SUBSCRIPTS: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];
    if n < SUBSCRIPTS.len() {
        SUBSCRIPTS[n]
    } else {
        char::from_digit(n as u32, 10).unwrap_or('?')
    }
}

/// Build the sort indicator suffix for a column header.
pub fn sort_indicator_string(state: &SortState, col_name: &str, multi: bool) -> String {
    if let Some(pos) = state.position(col_name) {
        let order = state.order_for(col_name).unwrap();
        let arrow = order.indicator();
        if multi {
            format!(" {}{}", arrow, subscript_digit(pos + 1))
        } else {
            format!(" {}", arrow)
        }
    } else {
        String::new()
    }
}

/// Compute column widths by sampling visible rows.
///
/// Returns `(headers, col_widths)` where:
/// - `headers[i]` is the display header string for column i
/// - `col_widths[i]` is the clamped column width in characters
///
/// `header_fn` controls header formatting per column (e.g. with sort indicators).
/// `sample_range` is the (start, end) row range in the batch to sample for data widths.
/// `max_width` is the maximum allowed column width.
pub fn compute_column_widths(
    schema: &Arc<Schema>,
    batch: &RecordBatch,
    formatters: &[Option<ArrayFormatter>],
    header_fn: &dyn Fn(usize, &str, &str) -> String,
    sample_range: (usize, usize),
    max_width: u16,
) -> (Vec<String>, Vec<u16>) {
    let fields = schema.fields();
    let mut headers = Vec::with_capacity(fields.len());
    let mut col_widths = Vec::with_capacity(fields.len());

    for (i, field) in fields.iter().enumerate() {
        let type_str = format!("{}", field.data_type());
        let header = header_fn(i, field.name(), &type_str);
        let header_width = header.width() as u16;

        let mut max_val_width: u16 = 0;
        if let Some(ref fmt) = formatters[i] {
            for row in sample_range.0..sample_range.1 {
                if row >= batch.num_rows() {
                    break;
                }
                let val = fmt.value(row).to_string();
                let w = val.width() as u16;
                if w > max_val_width {
                    max_val_width = w;
                }
            }
        }

        let width = header_width.max(max_val_width).clamp(4, max_width);
        headers.push(header);
        col_widths.push(width);
    }

    (headers, col_widths)
}

/// Determine which columns fit in the available width, starting from `column_offset`.
/// `left_margin` is the width used before the first column (e.g. 3 for row prefix ">> ").
/// `hidden` optionally specifies which columns are hidden and should be skipped.
pub fn visible_columns(
    col_widths: &[u16],
    available_width: usize,
    column_offset: usize,
    left_margin: usize,
    hidden: Option<&[bool]>,
) -> Vec<usize> {
    let mut visible_cols = Vec::new();
    let mut used_width = left_margin;

    for i in column_offset..col_widths.len() {
        if hidden.map_or(false, |h| h.get(i).copied().unwrap_or(false)) {
            continue;
        }
        let col_total = col_widths[i] as usize + 2;
        if used_width + col_total > available_width && !visible_cols.is_empty() {
            break;
        }
        visible_cols.push(i);
        used_width += col_total;
    }

    visible_cols
}

/// Build formatters for all columns in a batch.
pub fn build_formatters(batch: &RecordBatch) -> Vec<Option<ArrayFormatter<'_>>> {
    (0..batch.num_columns())
        .map(|i| {
            ArrayFormatter::try_new(batch.column(i).as_ref(), &Default::default()).ok()
        })
        .collect()
}

/// Truncate a string to fit within `width` characters, appending ellipsis if needed.
pub fn truncate_to_width(value: &str, width: usize) -> String {
    if value.width() <= width {
        format!("{:<width$}", value)
    } else {
        let mut s = String::new();
        let mut w = 0;
        for ch in value.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if w + cw > width.saturating_sub(1) {
                break;
            }
            s.push(ch);
            w += cw;
        }
        s.push('\u{2026}');
        s
    }
}

// ---------------------------------------------------------------------------
// TableStyler trait + UnifiedTable widget
// ---------------------------------------------------------------------------

/// Controls per-view differences in table rendering.
pub trait TableStyler {
    /// Override per-column header style (e.g., underline for diff key columns).
    fn col_header_style(&self, _col_idx: usize, base: Style) -> Style {
        base
    }

    /// Row prefix text and style (e.g., ">> " for selected, "+ " for diff).
    fn row_prefix(&self, data_row: usize) -> (&str, Style);

    /// Background style for the entire data row.
    fn row_bg(&self, _data_row: usize) -> Style {
        Style::default()
    }

    /// Cell display value and style. `formatted` is the default Arrow-formatted value.
    fn cell(
        &self,
        col_idx: usize,
        data_row: usize,
        formatted: &str,
        is_null: bool,
        base: Style,
    ) -> (String, Style);
}

/// Trivial styler: NULL in gray, no highlighting. Used by SQL results.
pub struct DefaultStyler;

impl TableStyler for DefaultStyler {
    fn row_prefix(&self, _data_row: usize) -> (&str, Style) {
        ("   ", Style::default())
    }

    fn cell(
        &self,
        _col_idx: usize,
        _data_row: usize,
        formatted: &str,
        is_null: bool,
        base: Style,
    ) -> (String, Style) {
        if is_null {
            ("NULL".to_string(), base.fg(Color::DarkGray))
        } else {
            (formatted.to_string(), base)
        }
    }
}

/// Unified table widget that delegates view-specific behavior to a `TableStyler`.
pub struct UnifiedTable<'a> {
    schema: &'a Arc<Schema>,
    batch: &'a RecordBatch,
    scroll_offset: usize,
    column_offset: usize,
    left_margin: usize,
    max_col_width: u16,
    show_types: bool,
    sort_state: Option<&'a SortState>,
    styler: &'a dyn TableStyler,
    /// Optional per-column minimum width overrides: (col_idx, min_width).
    col_width_mins: Option<&'a [(usize, u16)]>,
    /// Optional per-column hidden flags.
    hidden: Option<&'a [bool]>,
    /// Optional per-column max width caps.
    col_width_caps: Option<&'a [u16]>,
    /// When set, render 1-based row numbers in a left gutter.
    /// Value is (absolute_row_of_first_batch_row, total_rows) for width calculation.
    row_numbers: Option<(usize, usize)>,
}

impl<'a> UnifiedTable<'a> {
    pub fn new(
        schema: &'a Arc<Schema>,
        batch: &'a RecordBatch,
        styler: &'a dyn TableStyler,
    ) -> Self {
        Self {
            schema,
            batch,
            scroll_offset: 0,
            column_offset: 0,
            left_margin: 3,
            max_col_width: 50,
            show_types: true,
            sort_state: None,
            styler,
            col_width_mins: None,
            hidden: None,
            col_width_caps: None,
            row_numbers: None,
        }
    }

    pub fn scroll_offset(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    pub fn column_offset(mut self, offset: usize) -> Self {
        self.column_offset = offset;
        self
    }

    pub fn left_margin(mut self, margin: usize) -> Self {
        self.left_margin = margin;
        self
    }

    pub fn max_col_width(mut self, width: u16) -> Self {
        self.max_col_width = width;
        self
    }

    pub fn show_types(mut self, show: bool) -> Self {
        self.show_types = show;
        self
    }

    pub fn sort_state(mut self, state: &'a SortState) -> Self {
        self.sort_state = Some(state);
        self
    }

    /// Provide per-column minimum width overrides as (col_idx, min_width) pairs.
    pub fn col_width_mins(mut self, mins: &'a [(usize, u16)]) -> Self {
        self.col_width_mins = Some(mins);
        self
    }

    /// Provide per-column hidden flags.
    pub fn hidden(mut self, hidden: &'a [bool]) -> Self {
        self.hidden = Some(hidden);
        self
    }

    /// Provide per-column max width caps.
    pub fn col_width_caps(mut self, caps: &'a [u16]) -> Self {
        self.col_width_caps = Some(caps);
        self
    }

    /// Enable row numbers in a left gutter.
    /// `base` is the absolute 0-based row index of the first row in the batch.
    /// `total_rows` is the total dataset size (used to determine gutter width).
    pub fn row_numbers(mut self, base: usize, total_rows: usize) -> Self {
        self.row_numbers = Some((base, total_rows));
        self
    }
}

impl Widget for UnifiedTable<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width < 4 {
            return;
        }

        let batch_rows = self.batch.num_rows();
        let fields = self.schema.fields();
        let formatters = build_formatters(self.batch);

        let multi = self
            .sort_state
            .map_or(false, |s| s.specs().len() > 1);

        // Build header_fn based on show_types and sort_state
        let header_fn: Box<dyn Fn(usize, &str, &str) -> String> = if self.show_types {
            let sort_state = self.sort_state;
            Box::new(move |_i, name: &str, type_str: &str| {
                let sort_ind = match sort_state {
                    Some(ss) => sort_indicator_string(ss, name, multi),
                    None => String::new(),
                };
                format!("{} [{}]{}", name, type_str, sort_ind)
            })
        } else {
            Box::new(|_i, name: &str, _type_str: &str| name.to_string())
        };

        // Row number gutter width (0 when disabled).
        let (gutter_width, row_num_base) = if let Some((base, total)) = self.row_numbers {
            // Width = digits needed for max row number (1-based) + 1 space separator
            let digits = if total == 0 { 1 } else { ((total) as f64).log10() as usize + 1 };
            (digits + 1, base)
        } else {
            (0, 0)
        };
        let effective_left_margin = gutter_width + self.left_margin;

        let data_area_height = (area.height as usize).saturating_sub(1); // minus header row
        let sample_end = (self.scroll_offset + data_area_height).min(batch_rows);

        let (headers, mut col_widths) = compute_column_widths(
            self.schema,
            self.batch,
            &formatters,
            &*header_fn,
            (self.scroll_offset, sample_end),
            self.max_col_width,
        );

        // Apply per-column minimum width overrides
        if let Some(mins) = self.col_width_mins {
            for &(col_idx, min_w) in mins {
                if col_idx < col_widths.len() {
                    col_widths[col_idx] = col_widths[col_idx].max(min_w).clamp(4, self.max_col_width);
                }
            }
        }

        // Apply per-column max width caps
        if let Some(caps) = self.col_width_caps {
            for (i, width) in col_widths.iter_mut().enumerate() {
                if let Some(&cap) = caps.get(i) {
                    *width = (*width).min(cap).max(4);
                }
            }
        }

        let visible_cols = visible_columns(
            &col_widths,
            area.width as usize,
            self.column_offset,
            effective_left_margin,
            self.hidden,
        );

        if visible_cols.is_empty() {
            return;
        }

        let gutter_style = Style::default().fg(Color::DarkGray);
        let num_col_width = gutter_width.saturating_sub(1); // digits only, no separator

        // --- Render header row ---
        let header_y = area.y;
        let header_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        buf.set_string(
            area.x,
            header_y,
            " ".repeat(area.width as usize),
            Style::default(),
        );

        let mut x = area.x + effective_left_margin as u16;

        for &col_idx in &visible_cols {
            let field = &fields[col_idx];
            let header = &headers[col_idx];
            let width = col_widths[col_idx] as usize;

            let col_style = self.styler.col_header_style(col_idx, header_style);

            if !self.show_types || header.width() > width {
                // No rich rendering: just the plain header string (truncated)
                let display = if header.width() > width {
                    header.chars().take(width).collect::<String>()
                } else {
                    format!("{:<width$}", header, width = width)
                };
                buf.set_string(x, header_y, &display, col_style);
            } else {
                // Rich rendering: name (cyan bold) + " [" + type (colored) + "]" + sort indicator
                let name_part = field.name();
                buf.set_string(x, header_y, name_part, col_style);
                let mut cx = x + name_part.width() as u16;

                let type_str = format!("{}", field.data_type()).to_lowercase();
                let tc = type_color(field.data_type());
                let type_style = self.styler.col_header_style(
                    col_idx,
                    Style::default().fg(tc).add_modifier(Modifier::BOLD),
                );

                buf.set_string(cx, header_y, " [", col_style);
                cx += 2;
                buf.set_string(cx, header_y, &type_str, type_style);
                cx += type_str.width() as u16;
                buf.set_string(cx, header_y, "]", col_style);
                cx += 1;

                if let Some(ss) = self.sort_state {
                    let sort_ind = sort_indicator_string(ss, field.name(), multi);
                    if !sort_ind.is_empty() {
                        buf.set_string(cx, header_y, &sort_ind, col_style);
                        cx += sort_ind.width() as u16;
                    }
                }

                let used = (cx - x) as usize;
                if used < width {
                    buf.set_string(cx, header_y, " ".repeat(width - used), Style::default());
                }
            }

            x += col_widths[col_idx] + 2;
        }

        // --- Render data rows ---
        let data_start_y = header_y + 1;

        for display_row in 0..data_area_height {
            let row_y = data_start_y + display_row as u16;
            if row_y >= area.y + area.height {
                break;
            }

            let data_row = self.scroll_offset + display_row;

            if data_row >= batch_rows {
                buf.set_string(
                    area.x,
                    row_y,
                    " ".repeat(area.width as usize),
                    Style::default(),
                );
                continue;
            }

            let row_bg = self.styler.row_bg(data_row);

            // Clear line with row background
            buf.set_string(
                area.x,
                row_y,
                " ".repeat(area.width as usize),
                row_bg,
            );

            // Row number gutter
            if self.row_numbers.is_some() {
                let abs_row = row_num_base + data_row + 1; // 1-based
                let num_str = format!("{:>width$} ", abs_row, width = num_col_width);
                // Use the row background to detect selection and pick a readable color
                let num_style = if row_bg.bg == Some(Color::DarkGray) {
                    row_bg.fg(Color::Yellow)
                } else {
                    gutter_style
                };
                buf.set_string(area.x, row_y, &num_str, num_style);
            }

            // Row prefix (e.g. ">> " for selected row)
            let (prefix, prefix_style) = self.styler.row_prefix(data_row);
            buf.set_string(area.x + gutter_width as u16, row_y, prefix, prefix_style);

            // Cells
            let mut x = area.x + effective_left_margin as u16;
            for &col_idx in &visible_cols {
                let width = col_widths[col_idx] as usize;
                let column = self.batch.column(col_idx);
                let is_null = column.is_null(data_row);

                let formatted = if is_null {
                    String::new() // styler handles NULL display
                } else if let Some(ref fmt) = formatters[col_idx] {
                    fmt.value(data_row).to_string()
                } else {
                    "?".to_string()
                };

                let (display_val, cell_style) =
                    self.styler.cell(col_idx, data_row, &formatted, is_null, row_bg);

                buf.set_string(
                    x,
                    row_y,
                    &truncate_to_width(&display_val, width),
                    cell_style,
                );
                x += col_widths[col_idx] + 2;
            }
        }
    }
}
