use duckdb::arrow::compute::concat_batches;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::state::DiffMarker;
use crate::ui::table_render::{TableStyler, UnifiedTable};

pub struct DiffView<'a> {
    app: &'a App,
}

impl<'a> DiffView<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

/// Styler for diff view: marker-based prefix, changed-cell yellow, key cols cyan, common rows dim.
struct DiffStyler<'a> {
    markers: &'a [DiffMarker],
    changed_cells: &'a Vec<Vec<bool>>,
    key_col_indices: Vec<usize>,
    diff_col_indices: Vec<usize>,
    scroll_offset: usize,
}

impl TableStyler for DiffStyler<'_> {
    fn col_header_style(&self, col_idx: usize, base: Style) -> Style {
        if self.key_col_indices.contains(&col_idx) {
            base.add_modifier(Modifier::UNDERLINED)
        } else {
            base
        }
    }

    fn row_prefix(&self, data_row: usize) -> (&str, Style) {
        let abs_row = self.scroll_offset + data_row;
        if abs_row >= self.markers.len() {
            return ("   ", Style::default());
        }
        match self.markers[abs_row] {
            DiffMarker::OnlyA => (
                " + ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            DiffMarker::OnlyB => (
                " - ",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            DiffMarker::Changed => (
                " ~ ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            DiffMarker::Common => ("   ", Style::default().fg(Color::DarkGray)),
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
        let abs_row = self.scroll_offset + data_row;
        let marker = if abs_row < self.markers.len() {
            self.markers[abs_row]
        } else {
            DiffMarker::Common
        };

        let dim_style = Style::default().fg(Color::DarkGray);
        let base_style = match marker {
            DiffMarker::OnlyA => Style::default().fg(Color::Green),
            DiffMarker::OnlyB => Style::default().fg(Color::Red),
            DiffMarker::Changed => Style::default().fg(Color::White),
            DiffMarker::Common => dim_style,
        };

        if is_null {
            let style = if marker == DiffMarker::Common {
                dim_style
            } else {
                Style::default().fg(Color::DarkGray)
            };
            return ("NULL".to_string(), style);
        }

        let is_key = self.key_col_indices.contains(&col_idx);
        let is_diff = self.diff_col_indices.contains(&col_idx);

        let style = if marker == DiffMarker::Common {
            dim_style
        } else if is_key {
            Style::default().fg(Color::Cyan)
        } else if is_diff
            && marker == DiffMarker::Changed
            && abs_row < self.changed_cells.len()
            && col_idx < self.changed_cells[abs_row].len()
            && self.changed_cells[abs_row][col_idx]
        {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if !is_diff && !is_key {
            dim_style
        } else {
            base_style
        };

        (formatted.to_string(), style)
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

        // Scroll position indicator
        if visible_count > 0 {
            let pos_text = format!(" {}/{} ", diff.scroll_offset + 1, visible_count);
            let pos_x = area.x + area.width - pos_text.width() as u16;
            buf.set_string(pos_x, header_y, &pos_text, header_style);
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

        let styler = DiffStyler {
            markers: markers_ref,
            changed_cells: changed_ref,
            key_col_indices,
            diff_col_indices,
            scroll_offset,
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
