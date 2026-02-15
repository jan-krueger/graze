use crossterm::event::KeyCode;

use crate::app::{App, AppMode};

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}
