use crossterm::event::KeyCode;

use crate::app::{App, AppMode};
use crate::state::first_visible_col;

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    let setup = &app.diff.setup;
    if setup.columns.is_empty() {
        app.mode = AppMode::Normal;
        return;
    }
    let col_count = setup.columns.len();

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if app.diff.setup.cursor < col_count - 1 {
                app.diff.setup.cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.diff.setup.cursor > 0 {
                app.diff.setup.cursor -= 1;
            }
        }
        KeyCode::Char(' ') => {
            let cursor = app.diff.setup.cursor;
            app.diff.setup.selected[cursor] = !app.diff.setup.selected[cursor];
        }
        KeyCode::Enter => {
            // Apply: selected = hidden
            let hidden: Vec<bool> = app.diff.setup.selected.clone();
            let max_col = hidden.len().saturating_sub(1);
            app.tab_mut().hidden_columns = hidden;

            // Ensure selected_col is not on a hidden column
            let hidden_ref = &app.tab().hidden_columns;
            let sel = app.tab().viewport.selected_col;
            if hidden_ref.get(sel).copied().unwrap_or(false) {
                let new_col = first_visible_col(hidden_ref, max_col);
                app.tab_mut().viewport.selected_col = new_col;
            }
            // Same for column_offset
            let hidden_ref = &app.tab().hidden_columns;
            let off = app.tab().viewport.column_offset;
            if hidden_ref.get(off).copied().unwrap_or(false) {
                let new_off = first_visible_col(hidden_ref, max_col);
                app.tab_mut().viewport.column_offset = new_off;
            }

            let hidden_count = app.tab().hidden_columns.iter().filter(|&&h| h).count();
            if hidden_count > 0 {
                app.status_message = Some(format!("{hidden_count} column(s) hidden"));
            } else {
                app.status_message = Some("All columns visible".to_string());
            }
            app.mode = AppMode::Normal;
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}
