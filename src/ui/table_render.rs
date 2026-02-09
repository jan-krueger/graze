use std::sync::Arc;

use duckdb::arrow::datatypes::Schema;
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::arrow::util::display::ArrayFormatter;
use unicode_width::UnicodeWidthStr;

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
pub fn visible_columns(
    col_widths: &[u16],
    available_width: usize,
    column_offset: usize,
    left_margin: usize,
) -> Vec<usize> {
    let mut visible_cols = Vec::new();
    let mut used_width = left_margin;

    for i in column_offset..col_widths.len() {
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
