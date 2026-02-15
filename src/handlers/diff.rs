use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, AppMode};
use crate::state::DiffMarker;

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

pub(crate) fn handle_diff(app: &mut App, key: crossterm::event::KeyEvent) {
    let visible_rows = app.diff.visible_row_count();
    let page_size = app.tab().viewport.page_size;
    let half_page = (page_size / 2).max(1);

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if visible_rows > 0 {
                app.diff.scroll_offset = (app.diff.scroll_offset + 1).min(visible_rows - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.diff.scroll_offset = app.diff.scroll_offset.saturating_sub(1);
        }
        KeyCode::Char('g') => {
            app.diff.scroll_offset = 0;
        }
        KeyCode::Char('G') => {
            if visible_rows > 0 {
                app.diff.scroll_offset = visible_rows - 1;
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if visible_rows > 0 {
                app.diff.scroll_offset =
                    (app.diff.scroll_offset + half_page).min(visible_rows - 1);
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.diff.scroll_offset = app.diff.scroll_offset.saturating_sub(half_page);
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.diff.column_offset = app.diff.column_offset.saturating_sub(1);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(ref schema) = app.diff.schema {
                let max_col = schema.fields().len().saturating_sub(1);
                if app.diff.column_offset < max_col {
                    app.diff.column_offset += 1;
                }
            }
        }
        KeyCode::Char('n') => {
            if app.diff.hide_common {
                // All visible rows are diffs, just step forward
                if visible_rows > 0 {
                    app.diff.scroll_offset = if app.diff.scroll_offset + 1 < visible_rows {
                        app.diff.scroll_offset + 1
                    } else {
                        0 // wrap
                    };
                }
            } else {
                // Jump to next diff row (skip Common)
                let total = app.diff.markers.len();
                if total > 0 {
                    let start = app.diff.scroll_offset + 1;
                    for i in start..total {
                        if app.diff.markers[i] != DiffMarker::Common {
                            app.diff.scroll_offset = i;
                            return;
                        }
                    }
                    for i in 0..start.min(total) {
                        if app.diff.markers[i] != DiffMarker::Common {
                            app.diff.scroll_offset = i;
                            return;
                        }
                    }
                }
            }
        }
        KeyCode::Char('N') => {
            if app.diff.hide_common {
                if visible_rows > 0 {
                    app.diff.scroll_offset = if app.diff.scroll_offset > 0 {
                        app.diff.scroll_offset - 1
                    } else {
                        visible_rows - 1 // wrap
                    };
                }
            } else {
                let total = app.diff.markers.len();
                if total > 0 {
                    let start = app.diff.scroll_offset;
                    if start > 0 {
                        for i in (0..start).rev() {
                            if app.diff.markers[i] != DiffMarker::Common {
                                app.diff.scroll_offset = i;
                                return;
                            }
                        }
                    }
                    for i in (start..total).rev() {
                        if app.diff.markers[i] != DiffMarker::Common {
                            app.diff.scroll_offset = i;
                            return;
                        }
                    }
                }
            }
        }
        KeyCode::Char('c') => {
            app.diff.hide_common = !app.diff.hide_common;
            app.diff.rebuild_visible_rows();
            // Clamp scroll offset to new visible row count
            let visible = app.diff.visible_row_count();
            if visible > 0 {
                app.diff.scroll_offset = app.diff.scroll_offset.min(visible - 1);
            } else {
                app.diff.scroll_offset = 0;
            }
        }
        KeyCode::Esc => {
            app.diff.batch = None;
            app.diff.schema = None;
            app.diff.markers.clear();
            app.diff.changed_cells.clear();
            app.diff.error = None;
            app.diff.loading = false;
            app.diff.hide_common = false;
            app.diff.visible_rows.clear();
            app.status_message = None;
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}
