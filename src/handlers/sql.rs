use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, AppMode};
use crate::event::Action;

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        // Execute SQL: F5, Ctrl-e, or Ctrl-Enter
        KeyCode::F(5) => {
            execute_sql_pad(app);
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            execute_sql_pad(app);
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
            execute_sql_pad(app);
        }
        KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let (Some(batch), Some(schema)) = (app.sql.result.take(), app.sql.result_schema.take()) {
                app.add_tab_from_batch(batch, schema);
            }
        }
        KeyCode::Enter => {
            // Insert newline (max 5 lines)
            if app.sql.lines.len() < 5 {
                let current_line = &app.sql.lines[app.sql.cursor_row];
                let remainder = current_line[app.sql.cursor_col..].to_string();
                let kept = current_line[..app.sql.cursor_col].to_string();
                app.sql.lines[app.sql.cursor_row] = kept;
                app.sql.cursor_row += 1;
                app.sql.lines.insert(app.sql.cursor_row, remainder);
                app.sql.cursor_col = 0;
            }
        }
        KeyCode::Esc => {
            // Exit SQL mode, clear result, restore original view
            app.mode = AppMode::Normal;
            app.sql.result = None;
            app.sql.result_schema = None;
            app.sql.error = None;
            app.status_message = None;
        }
        KeyCode::Backspace => {
            if app.sql.cursor_col > 0 {
                app.sql.cursor_col -= 1;
                app.sql.lines[app.sql.cursor_row].remove(app.sql.cursor_col);
            } else if app.sql.cursor_row > 0 {
                // Join with previous line
                let current_line = app.sql.lines.remove(app.sql.cursor_row);
                app.sql.cursor_row -= 1;
                app.sql.cursor_col = app.sql.lines[app.sql.cursor_row].len();
                app.sql.lines[app.sql.cursor_row].push_str(&current_line);
            }
        }
        KeyCode::Left => {
            if app.sql.cursor_col > 0 {
                app.sql.cursor_col -= 1;
            } else if app.sql.cursor_row > 0 {
                app.sql.cursor_row -= 1;
                app.sql.cursor_col = app.sql.lines[app.sql.cursor_row].len();
            }
        }
        KeyCode::Right => {
            let line_len = app.sql.lines[app.sql.cursor_row].len();
            if app.sql.cursor_col < line_len {
                app.sql.cursor_col += 1;
            } else if app.sql.cursor_row + 1 < app.sql.lines.len() {
                app.sql.cursor_row += 1;
                app.sql.cursor_col = 0;
            }
        }
        KeyCode::Up => {
            if app.sql.cursor_row > 0 {
                app.sql.cursor_row -= 1;
                let line_len = app.sql.lines[app.sql.cursor_row].len();
                app.sql.cursor_col = app.sql.cursor_col.min(line_len);
            }
        }
        KeyCode::Down => {
            if app.sql.cursor_row + 1 < app.sql.lines.len() {
                app.sql.cursor_row += 1;
                let line_len = app.sql.lines[app.sql.cursor_row].len();
                app.sql.cursor_col = app.sql.cursor_col.min(line_len);
            }
        }
        KeyCode::Char(c) => {
            app.sql.lines[app.sql.cursor_row].insert(app.sql.cursor_col, c);
            app.sql.cursor_col += 1;
        }
        _ => {}
    }
}

fn execute_sql_pad(app: &mut App) {
    let sql = app.sql.lines.join("\n").trim().to_string();
    if !sql.is_empty() {
        app.sql.error = None;
        app.status_message = Some("Executing SQL...".to_string());
        app.send_action(Action::ExecuteSql(sql));
    }
}
