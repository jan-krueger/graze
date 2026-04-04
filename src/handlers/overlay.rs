use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::App;
use crate::mode::{AppMode, OverlayVariant};
use crate::state::first_visible_col;

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    let overlay = match &app.mode {
        AppMode::Overlay(v) => v.clone(),
        _ => return,
    };

    match overlay {
        OverlayVariant::Stats => handle_stats(app, key),
        OverlayVariant::Help => handle_help(app, key),
        OverlayVariant::ColumnPicker => handle_column_picker(app, key),
        OverlayVariant::ColumnJump => handle_column_jump(app, key),
    }
}

fn handle_stats(app: &mut App, key: crossterm::event::KeyEvent) {
    let total_rows = app.stats.batch.as_ref().map_or(0, |b| b.num_rows());
    // Visible data rows inside popup: popup content_height - 1 (for table header).
    // The popup content_height is roughly terminal_height - 12 (popup chrome + outer chrome).
    let visible_rows = app.stats.visible_rows.get().max(1);
    let max_scroll = total_rows.saturating_sub(visible_rows);
    let half_page = (visible_rows / 2).max(1);

    match key.code {
        KeyCode::Esc => {
            app.transition_to(AppMode::Normal);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if total_rows > 0 {
                app.stats.scroll_offset = (app.stats.scroll_offset + 1).min(max_scroll);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.stats.scroll_offset = app.stats.scroll_offset.saturating_sub(1);
        }
        KeyCode::Char('g') => {
            app.stats.scroll_offset = 0;
        }
        KeyCode::Char('G') => {
            app.stats.scroll_offset = max_scroll;
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if total_rows > 0 {
                app.stats.scroll_offset =
                    (app.stats.scroll_offset + half_page).min(max_scroll);
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.stats.scroll_offset = app.stats.scroll_offset.saturating_sub(half_page);
        }
        _ => {}
    }
}

fn handle_help(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.transition_to(AppMode::Normal);
        }
        _ => {}
    }
}

fn handle_column_picker(app: &mut App, key: crossterm::event::KeyEvent) {
    if app.column_picker.is_empty() {
        app.mode = AppMode::Normal;
        return;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.column_picker.move_down();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.column_picker.move_up();
        }
        KeyCode::Char(' ') => {
            app.column_picker.toggle_at_cursor();
        }
        KeyCode::Enter => {
            on_column_hide_confirm(app);
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_column_jump(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Down => {
            app.column_jump.move_down();
        }
        KeyCode::Up => {
            app.column_jump.move_up();
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.column_jump.move_down();
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.column_jump.move_up();
        }
        KeyCode::Char(c) => {
            app.column_jump.query.push(c);
            app.column_jump.refilter();
            app.column_jump.cursor = 0;
        }
        KeyCode::Backspace => {
            app.column_jump.query.pop();
            app.column_jump.refilter();
        }
        KeyCode::Enter => {
            if let Some(col_idx) = app.column_jump.selected_col_idx() {
                app.tab_mut().viewport.column_offset = col_idx;
                app.tab_mut().viewport.selected_col = col_idx;
                let name = app.column_jump.all_columns
                    .iter()
                    .find(|(i, _)| *i == col_idx)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default();
                app.transition_to(AppMode::Normal);
                app.status_message = Some(format!("Jumped to column: {name}"));
            }
        }
        KeyCode::Esc => {
            app.transition_to(AppMode::Normal);
        }
        _ => {}
    }
}

fn on_column_hide_confirm(app: &mut App) {
    let hidden: Vec<bool> = app.column_picker.selected.clone();
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
    app.transition_to(AppMode::Normal);
}
