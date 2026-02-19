use duckdb::arrow::compute::concat_batches;
use duckdb::arrow::util::display::ArrayFormatter;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::state::DiffMarker;
use crate::ui::table_render::{TableStyler, UnifiedTable};
use crate::ui::theme::Theme;

pub struct DiffView<'a> {
    app: &'a App,
}

impl<'a> DiffView<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

/// Styler for diff view: marker-based prefix, changed-cell yellow, key cols cyan, common rows dim.
///
/// NOTE: `data_row` passed by `UnifiedTable` is already offset by `scroll_offset`
/// (i.e. `data_row = scroll_offset + display_row`), so the styler must NOT add
/// scroll_offset again.
struct DiffStyler<'a> {
    markers: &'a [DiffMarker],
    changed_cells: &'a Vec<Vec<bool>>,
    key_col_indices: Vec<usize>,
    diff_col_indices: Vec<usize>,
    selected_col: usize,
    /// The data_row index of the cursor (selected_row in batch coordinates).
    selected_data_row: usize,
    theme: &'a Theme,
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
        if data_row >= self.markers.len() {
            return ("   ", Style::default());
        }
        match self.markers[data_row] {
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
        if data_row == self.selected_data_row {
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
        let marker = if data_row < self.markers.len() {
            self.markers[data_row]
        } else {
            DiffMarker::Common
        };

        let is_selected_row = data_row == self.selected_data_row;
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

        let mut style = if marker == DiffMarker::Common {
            dim_style
        } else if is_key {
            Style::default().fg(self.theme.header_fg)
        } else if is_diff
            && marker == DiffMarker::Changed
            && data_row < self.changed_cells.len()
            && col_idx < self.changed_cells[data_row].len()
            && self.changed_cells[data_row][col_idx]
        {
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
fn build_cell_preview(app: &App) -> Option<String> {
    let diff = &app.diff;
    let data_row = diff.display_to_data_row(diff.selected_row);
    let col = diff.selected_col;

    // Check if this cell is actually changed
    let is_changed = diff
        .changed_cells
        .get(data_row)
        .and_then(|row| row.get(col))
        .copied()
        .unwrap_or(false);
    if !is_changed {
        return None;
    }

    let schema = diff.schema.as_ref()?;
    let batch = diff.batch.as_ref()?;
    let b_side_batch = diff.b_side_batch.as_ref()?;
    let b_side_idx = diff.b_side_col_map.get(col)?.as_ref().copied()?;

    if data_row >= batch.num_rows() {
        return None;
    }

    let col_name = schema.fields()[col].name();

    // Get B-side (old) value
    let b_col = b_side_batch.column(b_side_idx);
    let old_val = if b_col.is_null(data_row) {
        "NULL".to_string()
    } else {
        ArrayFormatter::try_new(b_col.as_ref(), &Default::default())
            .ok()
            .map(|f| f.value(data_row).to_string())
            .unwrap_or_default()
    };

    // Get A-side (new) value
    let a_col = batch.column(col);
    let new_val = if a_col.is_null(data_row) {
        "NULL".to_string()
    } else {
        ArrayFormatter::try_new(a_col.as_ref(), &Default::default())
            .ok()
            .map(|f| f.value(data_row).to_string())
            .unwrap_or_default()
    };

    Some(format!("{}: \"{}\" → \"{}\"", col_name, old_val, new_val))
}

impl Widget for DiffView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

        let diff = &self.app.diff;
        let theme = &self.app.theme;

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
            let spinner = self.app.spinner_char();
            let msg = format!("{} Computing diff...", spinner);
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
        let full_batch = match diff.batch.as_ref() {
            Some(b) => b,
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
            let preview = build_cell_preview(self.app);
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

        // When hide_common is on, build a filtered batch and markers
        let (render_batch, render_markers, render_changed_cells);
        let (batch_ref, markers_ref, changed_ref, scroll_offset);

        if diff.hide_common && !diff.visible_rows.is_empty() {
            let slices: Vec<_> = diff.visible_rows.iter().map(|&i| full_batch.slice(i, 1)).collect();
            render_batch = concat_batches(schema, &slices).unwrap_or_else(|_| full_batch.clone());
            render_markers = diff.visible_rows.iter().map(|&i| diff.markers[i]).collect::<Vec<_>>();
            render_changed_cells = diff.visible_rows.iter().map(|&i| {
                diff.changed_cells.get(i).cloned().unwrap_or_default()
            }).collect::<Vec<_>>();
            batch_ref = &render_batch;
            markers_ref = &render_markers;
            changed_ref = &render_changed_cells;
            scroll_offset = diff.scroll_offset;
        } else {
            batch_ref = full_batch;
            markers_ref = &diff.markers;
            changed_ref = &diff.changed_cells;
            scroll_offset = diff.scroll_offset;
        };

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

        // selected_row in batch/render coordinates (for row_bg highlight)
        let selected_data_row = diff.selected_row;

        let styler = DiffStyler {
            markers: markers_ref,
            changed_cells: changed_ref,
            key_col_indices,
            diff_col_indices,
            selected_col: diff.selected_col,
            selected_data_row,
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
