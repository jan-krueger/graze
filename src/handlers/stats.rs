use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, AppMode};

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    let total_rows = app.stats.batch.as_ref().map_or(0, |b| b.num_rows());
    let half_page = (app.viewport.page_size / 4).max(1);

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.stats.batch = None;
            app.stats.schema = None;
            app.stats.loading = false;
            app.status_message = None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if total_rows > 0 {
                app.stats.scroll_offset = (app.stats.scroll_offset + 1).min(total_rows - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.stats.scroll_offset = app.stats.scroll_offset.saturating_sub(1);
        }
        KeyCode::Char('g') => {
            app.stats.scroll_offset = 0;
        }
        KeyCode::Char('G') => {
            if total_rows > 0 {
                app.stats.scroll_offset = total_rows - 1;
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if total_rows > 0 {
                app.stats.scroll_offset =
                    (app.stats.scroll_offset + half_page).min(total_rows - 1);
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.stats.scroll_offset = app.stats.scroll_offset.saturating_sub(half_page);
        }
        _ => {}
    }
}
