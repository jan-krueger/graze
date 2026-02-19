use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, AppMode};

pub(crate) fn handle_setup_key(app: &mut App, key: crossterm::event::KeyEvent) {
    let col_count = app.diff.setup.columns.len();
    if col_count == 0 {
        app.mode = AppMode::Normal;
        return;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if app.diff.setup.cursor + 1 < col_count {
                app.diff.setup.cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.diff.setup.cursor = app.diff.setup.cursor.saturating_sub(1);
        }
        KeyCode::Char(' ') => {
            let idx = app.diff.setup.cursor;
            app.diff.setup.selected[idx] = !app.diff.setup.selected[idx];
        }
        KeyCode::Enter => {
            let key_columns: Vec<String> = app
                .diff
                .setup
                .columns
                .iter()
                .zip(app.diff.setup.selected.iter())
                .filter(|(_, sel)| **sel)
                .map(|(name, _)| name.clone())
                .collect();

            if key_columns.is_empty() {
                app.status_message = Some("Select at least one key column".to_string());
                return;
            }

            app.diff.key_columns = key_columns.clone();

            // Reset setup for diff column picker: pre-select all non-key columns
            let columns = app.diff.setup.columns.clone();
            let selected: Vec<bool> = columns
                .iter()
                .map(|c| !key_columns.contains(c))
                .collect();
            app.diff.setup.selected = selected;
            app.diff.setup.cursor = 0;
            app.mode = AppMode::DiffSetupCols;
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

pub(crate) fn handle_setup_cols(app: &mut App, key: crossterm::event::KeyEvent) {
    let col_count = app.diff.setup.columns.len();
    if col_count == 0 {
        app.mode = AppMode::Normal;
        return;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if app.diff.setup.cursor + 1 < col_count {
                app.diff.setup.cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.diff.setup.cursor = app.diff.setup.cursor.saturating_sub(1);
        }
        KeyCode::Char(' ') => {
            let idx = app.diff.setup.cursor;
            app.diff.setup.selected[idx] = !app.diff.setup.selected[idx];
        }
        KeyCode::Enter => {
            let diff_columns: Vec<String> = app
                .diff
                .setup
                .columns
                .iter()
                .zip(app.diff.setup.selected.iter())
                .filter(|(_, sel)| **sel)
                .map(|(name, _)| name.clone())
                .collect();

            if diff_columns.is_empty() {
                app.status_message = Some("Select at least one column to compare".to_string());
                return;
            }

            app.diff.diff_columns = diff_columns;
            app.start_diff();
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

/// Ensure selected_col is visible by placing it at the left edge of the viewport.
/// Used by n/N jumps where the target column may be far from current view.
fn scroll_to_selected_col(app: &mut App) {
    app.diff.column_offset = app.diff.selected_col;
}

/// Compute the diff view page size (data rows visible in the table area).
/// Layout: cyan header (1) + column header (1) = 2 rows of chrome within the diff area.
fn diff_page_size(app: &App) -> usize {
    // The diff view area is the full main area minus the status bar (1 row).
    // Inside it: cyan header bar (1) + UnifiedTable column header (1) = 2 chrome rows.
    // The tab bar is already subtracted from the terminal height in update_terminal_size.
    let chrome = 3; // cyan bar + column header + status bar
    let tab_chrome = if app.has_tabs() { 1 } else { 0 };
    let term_height = app.tab().viewport.page_size + 2 + tab_chrome; // reverse the normal chrome calc
    term_height.saturating_sub(chrome + tab_chrome).max(1)
}

/// Jump to a specific (data_row, col) in diff view, adjusting selected_row and selected_col.
/// Handles hide_common mapping from data row to display row.
fn jump_to_diff_cell(app: &mut App, data_row: usize, col: usize) {
    let display_row = if app.diff.hide_common {
        app.diff
            .visible_rows
            .iter()
            .position(|&r| r == data_row)
            .unwrap_or(0)
    } else {
        data_row
    };
    app.diff.selected_row = display_row;
    app.diff.selected_col = col;
    let page = diff_page_size(app);
    app.diff.adjust_view(page);
    scroll_to_selected_col(app);
    app.ensure_diff_page();
}

pub(crate) fn handle_diff(app: &mut App, key: crossterm::event::KeyEvent) {
    let visible_rows = app.diff.visible_row_count();
    let page_size = diff_page_size(app);
    let half_page = (page_size / 2).max(1);

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if visible_rows > 0 {
                app.diff.selected_row = (app.diff.selected_row + 1).min(visible_rows - 1);
                app.diff.adjust_view(page_size);
                app.ensure_diff_page();
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.diff.selected_row = app.diff.selected_row.saturating_sub(1);
            app.diff.adjust_view(page_size);
            app.ensure_diff_page();
        }
        KeyCode::Char('g') => {
            app.diff.selected_row = 0;
            app.diff.adjust_view(page_size);
            app.ensure_diff_page();
        }
        KeyCode::Char('G') => {
            if visible_rows > 0 {
                app.diff.selected_row = visible_rows - 1;
                app.diff.adjust_view(page_size);
                app.ensure_diff_page();
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if visible_rows > 0 {
                app.diff.selected_row =
                    (app.diff.selected_row + half_page).min(visible_rows - 1);
                app.diff.adjust_view(page_size);
                app.ensure_diff_page();
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.diff.selected_row = app.diff.selected_row.saturating_sub(half_page);
            app.diff.adjust_view(page_size);
            app.ensure_diff_page();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            if app.diff.selected_col > 0 {
                app.diff.selected_col -= 1;
                if app.diff.selected_col < app.diff.column_offset {
                    app.diff.column_offset = app.diff.selected_col;
                }
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(ref schema) = app.diff.schema {
                let max_col = schema.fields().len().saturating_sub(1);
                if app.diff.selected_col < max_col {
                    app.diff.selected_col += 1;
                    if app.diff.column_offset < app.diff.selected_col {
                        app.diff.column_offset += 1;
                    }
                }
            }
        }
        KeyCode::Char('n') => {
            // Use changed_row_indices for cell-level n/N navigation
            nav_next_changed(app, visible_rows, page_size);
        }
        KeyCode::Char('N') => {
            nav_prev_changed(app, visible_rows, page_size);
        }
        KeyCode::Char('c') => {
            app.diff.hide_common = !app.diff.hide_common;
            app.diff.rebuild_visible_rows();
            let visible = app.diff.visible_row_count();
            if visible > 0 {
                app.diff.selected_row = app.diff.selected_row.min(visible - 1);
            } else {
                app.diff.selected_row = 0;
            }
            app.diff.adjust_view(page_size);
            app.ensure_diff_page();
        }
        KeyCode::Esc => {
            app.diff.backend = None;
            app.diff.page = None;
            app.diff.schema = None;
            app.diff.markers.clear();
            app.diff.changed_row_indices.clear();
            app.diff.total_rows = 0;
            app.diff.error = None;
            app.diff.loading = false;
            app.diff.hide_common = false;
            app.diff.visible_rows.clear();
            app.diff.b_side_col_map.clear();
            app.diff.selected_col = 0;
            app.diff.selected_row = 0;
            app.diff.scroll_offset = 0;
            app.status_message = None;
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

/// Jump to next non-Common row using changed_row_indices binary search.
fn nav_next_changed(app: &mut App, visible_rows: usize, page_size: usize) {
    if app.diff.changed_row_indices.is_empty() {
        return;
    }

    let current_data_row = app.diff.display_to_data_row(app.diff.selected_row);

    // Find the first changed row after current_data_row
    let idx = match app.diff.changed_row_indices.binary_search(&(current_data_row + 1)) {
        Ok(i) => Some(i),
        Err(i) => {
            if i < app.diff.changed_row_indices.len() {
                Some(i)
            } else {
                // Wrap to beginning
                Some(0)
            }
        }
    };

    if let Some(i) = idx {
        let data_row = app.diff.changed_row_indices[i];
        jump_to_diff_cell(app, data_row, app.diff.selected_col);
    }

    app.diff.adjust_view(page_size);
    let _ = visible_rows; // used for type consistency
}

/// Jump to previous non-Common row using changed_row_indices binary search.
fn nav_prev_changed(app: &mut App, visible_rows: usize, page_size: usize) {
    if app.diff.changed_row_indices.is_empty() {
        return;
    }

    let current_data_row = app.diff.display_to_data_row(app.diff.selected_row);

    // Find the last changed row before current_data_row
    let idx = match app.diff.changed_row_indices.binary_search(&current_data_row) {
        Ok(i) => {
            if i > 0 {
                Some(i - 1)
            } else {
                // Wrap to end
                Some(app.diff.changed_row_indices.len() - 1)
            }
        }
        Err(i) => {
            if i > 0 {
                Some(i - 1)
            } else {
                // Wrap to end
                Some(app.diff.changed_row_indices.len() - 1)
            }
        }
    };

    if let Some(i) = idx {
        let data_row = app.diff.changed_row_indices[i];
        jump_to_diff_cell(app, data_row, app.diff.selected_col);
    }

    app.diff.adjust_view(page_size);
    let _ = visible_rows;
}
