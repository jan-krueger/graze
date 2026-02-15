use crossterm::event::KeyCode;

use crate::app::{App, AppMode};

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char(c) if c.is_ascii_digit() => {
            app.tab_mut().filter.insert_at_cursor(c);
        }
        KeyCode::Backspace => {
            app.tab_mut().filter.delete_before_cursor();
        }
        KeyCode::Left => {
            app.tab_mut().filter.move_cursor_left();
        }
        KeyCode::Right => {
            app.tab_mut().filter.move_cursor_right();
        }
        KeyCode::Home => {
            app.tab_mut().filter.move_cursor_to_start();
        }
        KeyCode::End => {
            app.tab_mut().filter.move_cursor_to_end();
        }
        KeyCode::Enter => {
            let input = app.tab().filter.input.clone();
            if let Ok(row_num) = input.parse::<usize>() {
                let total = app.tab().data.total_rows;
                if row_num >= 1 && (total == 0 || row_num <= total) {
                    app.tab_mut().viewport.selected_row = row_num - 1;
                    app.tab_mut().viewport.adjust_view();
                    app.ensure_buffer();
                    app.status_message = Some(format!("Jumped to row {row_num}"));
                    app.tab_mut().filter.input.clear();
                    app.tab_mut().filter.cursor_pos = 0;
                    app.mode = AppMode::Normal;
                } else {
                    app.status_message =
                        Some(format!("Row must be between 1 and {total}"));
                }
            } else if input.is_empty() {
                app.status_message = Some("Enter a row number".to_string());
            } else {
                app.status_message = Some("Invalid row number".to_string());
            }
        }
        KeyCode::Esc => {
            app.tab_mut().filter.input.clear();
            app.tab_mut().filter.cursor_pos = 0;
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}
