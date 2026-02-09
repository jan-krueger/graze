use crate::app::App;
use crate::state::SelectionMode;

/// Jump to the next cell containing the active search term.
pub(crate) fn jump_to_next_match(app: &mut App) {
    if let Some(target) = find_match(app, true) {
        app.viewport.selected_row = app.viewport.view_start + target.0;
        app.viewport.selected_col = target.1;
        app.viewport.selection_mode = SelectionMode::Column;
        app.viewport.adjust_view();
        app.adjust_column_view();
    }
}

/// Jump to the previous cell containing the active search term.
pub(crate) fn jump_to_prev_match(app: &mut App) {
    if let Some(target) = find_match(app, false) {
        app.viewport.selected_row = app.viewport.view_start + target.0;
        app.viewport.selected_col = target.1;
        app.viewport.selection_mode = SelectionMode::Column;
        app.viewport.adjust_view();
        app.adjust_column_view();
    }
}

/// Find the next or previous search match in visible cells.
fn find_match(app: &App, forward: bool) -> Option<(usize, usize)> {
    let term = match app.search.active_search.as_ref() {
        Some(t) => t.to_lowercase(),
        None => return None,
    };
    let batch = match app.data.current_batch.as_ref() {
        Some(b) => b,
        None => return None,
    };
    let schema = match app.data.schema.as_ref() {
        Some(s) => s,
        None => return None,
    };

    let num_cols = schema.fields().len();
    if num_cols == 0 {
        return None;
    }

    let view_off = app.view_offset_in_buffer();
    let visible_count = app.visible_row_count();
    if visible_count == 0 {
        return None;
    }

    let batch_rows = batch.num_rows();

    let formatters: Vec<Option<duckdb::arrow::util::display::ArrayFormatter>> =
        (0..batch.num_columns())
            .map(|i| {
                duckdb::arrow::util::display::ArrayFormatter::try_new(
                    batch.column(i).as_ref(),
                    &Default::default(),
                )
                .ok()
            })
            .collect();

    let total_cells = visible_count * num_cols;
    let row_in_view = app
        .viewport
        .selected_row
        .saturating_sub(app.viewport.view_start);
    let start_pos = row_in_view * num_cols + app.viewport.selected_col;

    for offset in 1..=total_cells {
        let pos = if forward {
            (start_pos + offset) % total_cells
        } else {
            (start_pos + total_cells - offset) % total_cells
        };
        let r = pos / num_cols;
        let c = pos % num_cols;
        let batch_row = view_off + r;
        if batch_row >= batch_rows {
            continue;
        }
        if c >= formatters.len() {
            continue;
        }
        let column = batch.column(c);
        if column.is_null(batch_row) {
            continue;
        }
        if let Some(ref fmt) = formatters[c] {
            let val = fmt.value(batch_row).to_string();
            if val.to_lowercase().contains(&term) {
                return Some((r, c));
            }
        }
    }

    None
}
