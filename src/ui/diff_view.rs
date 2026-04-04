use duckdb::arrow::util::display::ArrayFormatter;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::diff::DiffPageData;
use crate::state::{DiffMarker, DiffState};
use crate::ui::table_render::{TableStyler, UnifiedTable};
use crate::ui::theme::Theme;

pub struct DiffView<'a> {
    diff: &'a DiffState,
    theme: &'a Theme,
    spinner_char: char,
}

impl<'a> DiffView<'a> {
    pub fn new(diff: &'a DiffState, theme: &'a Theme, spinner_char: char) -> Self {
        Self { diff, theme, spinner_char }
    }
}

/// Styler for diff view: marker-based prefix, changed-cell yellow, key cols cyan, common rows dim.
///
/// In the paginated design, `data_row` passed by `UnifiedTable` is an offset into the
/// render batch. We use `row_indices` to map it back to the original data row for
/// marker/changed_cells lookups.
struct DiffStyler<'a> {
    markers: &'a [DiffMarker],
    changed_cells: &'a [Vec<bool>],
    /// Maps render batch row → data row index.
    row_indices: &'a [usize],
    key_col_indices: Vec<usize>,
    diff_col_indices: Vec<usize>,
    selected_col: usize,
    /// The render batch row index of the cursor.
    selected_batch_row: usize,
    theme: &'a Theme,
}

impl DiffStyler<'_> {
    /// Map a render batch row to its data row index.
    fn data_row(&self, batch_row: usize) -> usize {
        self.row_indices.get(batch_row).copied().unwrap_or(0)
    }
}

impl TableStyler for DiffStyler<'_> {
    fn col_header_style(&self, col_idx: usize, base: Style) -> Style {
        if col_idx == self.selected_col {
            base.add_modifier(Modifier::REVERSED)
        } else if self.key_col_indices.contains(&col_idx) {
            base.add_modifier(Modifier::UNDERLINED)
        } else {
            base
        }
    }

    fn row_prefix(&self, data_row: usize) -> (&str, Style) {
        let actual = self.data_row(data_row);
        if actual >= self.markers.len() {
            return ("   ", Style::default());
        }
        match self.markers[actual] {
            DiffMarker::OnlyA => (
                " + ",
                Style::default()
                    .fg(self.theme.diff_added)
                    .add_modifier(Modifier::BOLD),
            ),
            DiffMarker::OnlyB => (
                " - ",
                Style::default()
                    .fg(self.theme.diff_removed)
                    .add_modifier(Modifier::BOLD),
            ),
            DiffMarker::Changed => (
                " ~ ",
                Style::default()
                    .fg(self.theme.diff_changed)
                    .add_modifier(Modifier::BOLD),
            ),
            DiffMarker::Common => ("   ", Style::default().fg(self.theme.dim)),
        }
    }

    fn row_bg(&self, data_row: usize) -> Style {
        if data_row == self.selected_batch_row {
            Style::default().bg(self.theme.selected_bg)
        } else {
            Style::default()
        }
    }

    fn cell(
        &self,
        col_idx: usize,
        data_row: usize,
        formatted: &str,
        is_null: bool,
        _base: Style,
    ) -> (String, Style) {
        let actual = self.data_row(data_row);
        let marker = if actual < self.markers.len() {
            self.markers[actual]
        } else {
            DiffMarker::Common
        };

        let is_selected_row = data_row == self.selected_batch_row;
        let dim_style = Style::default().fg(self.theme.dim);
        let base_style = match marker {
            DiffMarker::OnlyA => Style::default().fg(self.theme.diff_added),
            DiffMarker::OnlyB => Style::default().fg(self.theme.diff_removed),
            DiffMarker::Changed => Style::default().fg(self.theme.fg),
            DiffMarker::Common => dim_style,
        };

        if is_null {
            let style = if marker == DiffMarker::Common {
                dim_style
            } else {
                Style::default().fg(self.theme.null_fg)
            };
            let style = if is_selected_row { style.bg(self.theme.selected_bg) } else { style };
            return ("NULL".to_string(), style);
        }

        let is_key = self.key_col_indices.contains(&col_idx);
        let is_diff = self.diff_col_indices.contains(&col_idx);

        // Use batch-local changed_cells (indexed by batch row, not data row)
        let cell_changed = is_diff
            && marker == DiffMarker::Changed
            && data_row < self.changed_cells.len()
            && col_idx < self.changed_cells[data_row].len()
            && self.changed_cells[data_row][col_idx];

        let mut style = if marker == DiffMarker::Common {
            dim_style
        } else if is_key {
            Style::default().fg(self.theme.header_fg)
        } else if cell_changed {
            Style::default()
                .fg(self.theme.diff_changed)
                .add_modifier(Modifier::BOLD)
        } else if !is_diff && !is_key {
            dim_style
        } else {
            base_style
        };

        if is_selected_row {
            style = style.bg(self.theme.selected_bg);
        }

        (formatted.to_string(), style)
    }
}

/// Build the "old → new" preview string for the current cell, if it's a changed cell.
pub fn build_cell_preview(diff: &DiffState) -> Option<String> {
    let data_row = diff.display_to_data_row(diff.selected_row);
    let col = diff.selected_col;

    let schema = diff.schema.as_ref()?;
    let page = diff.page.as_ref()?;

    // Find this data_row in the page
    let batch_row = page.row_indices.iter().position(|&r| r == data_row)?;

    // Check if this cell is actually changed
    let is_changed = page
        .changed_cells
        .get(batch_row)
        .and_then(|row| row.get(col))
        .copied()
        .unwrap_or(false);
    if !is_changed {
        return None;
    }

    let b_side_idx = diff.b_side_col_map.get(col)?.as_ref().copied()?;

    if batch_row >= page.display_batch.num_rows() {
        return None;
    }

    let col_name = schema.fields()[col].name();

    // Get B-side (old) value
    let b_col = page.b_side_batch.column(b_side_idx);
    let old_val = if b_col.is_null(batch_row) {
        "NULL".to_string()
    } else {
        ArrayFormatter::try_new(b_col.as_ref(), &Default::default())
            .ok()
            .map(|f| f.value(batch_row).to_string())
            .unwrap_or_default()
    };

    // Get A-side (new) value
    let a_col = page.display_batch.column(col);
    let new_val = if a_col.is_null(batch_row) {
        "NULL".to_string()
    } else {
        ArrayFormatter::try_new(a_col.as_ref(), &Default::default())
            .ok()
            .map(|f| f.value(batch_row).to_string())
            .unwrap_or_default()
    };

    Some(format!("{}: \"{}\" → \"{}\"", col_name, old_val, new_val))
}

/// Given the page data, extract the subset of rows that should be rendered
/// based on scroll_offset and available height, returning a (batch, changed_cells, row_indices)
/// tuple that is ready for the UnifiedTable.
fn build_render_data<'a>(
    diff: &'a DiffState,
    page: &'a DiffPageData,
) -> (
    &'a duckdb::arrow::record_batch::RecordBatch,
    &'a [Vec<bool>],
    &'a [usize],
    usize, // scroll_offset within the render batch
) {
    let viewport_start_data_row = diff.display_to_data_row(diff.scroll_offset);

    let page_scroll = page
        .row_indices
        .iter()
        .position(|&r| r >= viewport_start_data_row)
        .unwrap_or(0);

    (
        &page.display_batch,
        &page.changed_cells,
        &page.row_indices,
        page_scroll,
    )
}

impl Widget for DiffView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

        let diff = self.diff;
        let theme = self.theme;

        // Header bar (1 row)
        let header_y = area.y;
        let header_style = Style::default()
            .bg(theme.diff_header_bg)
            .fg(theme.diff_header_fg)
            .add_modifier(Modifier::BOLD);

        buf.set_string(
            area.x,
            header_y,
            " ".repeat(area.width as usize),
            header_style,
        );

        if diff.loading {
            let msg = format!("{} Computing diff...", self.spinner_char);
            buf.set_string(area.x + 1, header_y, &msg, header_style);
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
        let page = match diff.page.as_ref() {
            Some(p) => p,
            None => return,
        };

        let visible_count = diff.visible_row_count();

        // Header text
        let hide_label = if diff.hide_common { " [changes only]" } else { "" };
        let header_text = format!(
            " Diff: {} vs {} | +{} -{} ~{} ={}{}",
            diff.file_a,
            diff.file_b,
            diff.counts.only_a,
            diff.counts.only_b,
            diff.counts.changed,
            diff.counts.common,
            hide_label,
        );
        buf.set_string(area.x, header_y, &header_text, header_style);

        // Right side of header: cell preview or position indicator
        if visible_count > 0 {
            let preview = build_cell_preview(diff);
            let right_text = if let Some(ref pv) = preview {
                let pos = format!("{}/{}", diff.selected_row + 1, visible_count);
                format!(" {} | {} ", pv, pos)
            } else {
                format!(" {}/{} ", diff.selected_row + 1, visible_count)
            };
            let right_w = right_text.width() as u16;
            if right_w < area.width {
                let pos_x = area.x + area.width - right_w;
                buf.set_string(pos_x, header_y, &right_text, header_style);
            }
        }

        let fields = schema.fields();
        if fields.is_empty() || visible_count == 0 {
            return;
        }

        let (batch_ref, changed_ref, row_indices, scroll_offset) =
            build_render_data(diff, page);

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

        // selected_row in render batch coordinates
        let selected_data_row = diff.display_to_data_row(diff.selected_row);
        let selected_batch_row = row_indices
            .iter()
            .position(|&r| r == selected_data_row)
            .unwrap_or(0);

        let styler = DiffStyler {
            markers: &diff.markers,
            changed_cells: changed_ref,
            row_indices,
            key_col_indices,
            diff_col_indices,
            selected_col: diff.selected_col,
            selected_batch_row,
            theme,
        };

        // Table area starts below the cyan header bar
        let table_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(1),
        };

        UnifiedTable::new(schema, batch_ref, &styler)
            .scroll_offset(scroll_offset)
            .column_offset(diff.column_offset)
            .render(table_area, buf);
    }
}
