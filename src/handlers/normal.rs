use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, AppMode};
use crate::event::Action;
use crate::state::SelectionMode;

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('q') => {
            app.mode = AppMode::Quitting;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.mode = AppMode::Quitting;
        }

        // Toggle selection mode
        KeyCode::Tab => {
            app.viewport.selection_mode = match app.viewport.selection_mode {
                SelectionMode::Row => {
                    app.viewport.selected_col = app.viewport.column_offset;
                    SelectionMode::Column
                }
                SelectionMode::Column => SelectionMode::Row,
            };
        }

        // Vertical navigation
        KeyCode::Char('j') | KeyCode::Down => app.move_down(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(1),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.move_down(app.viewport.page_size / 2);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.move_up(app.viewport.page_size / 2);
        }
        KeyCode::Char('g') => {
            app.viewport.selected_row = 0;
            app.viewport.adjust_view();
            app.ensure_buffer();
        }
        KeyCode::Char('G') => {
            if app.data.total_rows > 0 {
                app.viewport.selected_row = app.data.total_rows - 1;
                app.viewport.adjust_view();
                app.ensure_buffer();
            }
        }

        // Horizontal navigation
        KeyCode::Char('h') | KeyCode::Left => {
            match app.viewport.selection_mode {
                SelectionMode::Row => {
                    if app.viewport.column_offset > 0 {
                        app.viewport.column_offset -= 1;
                    }
                }
                SelectionMode::Column => {
                    if app.viewport.selected_col > 0 {
                        app.viewport.selected_col -= 1;
                        app.adjust_column_view();
                    }
                }
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(ref schema) = app.data.schema {
                let max_col = schema.fields().len().saturating_sub(1);
                match app.viewport.selection_mode {
                    SelectionMode::Row => {
                        if app.viewport.column_offset < max_col {
                            app.viewport.column_offset += 1;
                        }
                    }
                    SelectionMode::Column => {
                        if app.viewport.selected_col < max_col {
                            app.viewport.selected_col += 1;
                            app.adjust_column_view();
                        }
                    }
                }
            }
        }
        KeyCode::Char('0') => {
            match app.viewport.selection_mode {
                SelectionMode::Row => {
                    app.viewport.column_offset = 0;
                }
                SelectionMode::Column => {
                    app.viewport.selected_col = 0;
                    app.adjust_column_view();
                }
            }
        }
        KeyCode::Char('$') => {
            if let Some(ref schema) = app.data.schema {
                let len = schema.fields().len();
                if len > 0 {
                    match app.viewport.selection_mode {
                        SelectionMode::Row => {
                            app.viewport.column_offset = len - 1;
                        }
                        SelectionMode::Column => {
                            app.viewport.selected_col = len - 1;
                            app.adjust_column_view();
                        }
                    }
                }
            }
        }

        // Sort
        KeyCode::Char('s') => app.cycle_sort(),

        // Stats overlay
        KeyCode::Char('S') => {
            app.mode = AppMode::Stats;
            app.stats.loading = true;
            app.stats.batch = None;
            app.stats.schema = None;
            app.stats.scroll_offset = 0;
            let table = app
                .data
                .table_name
                .as_deref()
                .unwrap_or("data")
                .to_string();
            app.status_message = Some("Loading statistics...".to_string());
            app.send_action(Action::ExecuteSql(format!("SUMMARIZE {table}")));
        }

        // Filter
        KeyCode::Char('/') => {
            app.mode = AppMode::Filter;
            app.filter.input.clear();
        }
        KeyCode::Char('f') => {
            // Column-specific filter shortcut: pre-fill with current column name
            if let Some(ref schema) = app.data.schema {
                let fields = schema.fields();
                if !fields.is_empty() {
                    let col_idx = app.viewport.selected_col.min(fields.len() - 1);
                    let col_name = fields[col_idx].name();
                    app.filter.input = format!("\"{}\" = ", col_name);
                    app.mode = AppMode::Filter;
                }
            }
        }
        KeyCode::Esc => {
            let had_search = app.search.active_search.is_some();
            let had_filter = app.filter.active_filter.is_some();

            // Clear both at once
            if had_search {
                app.search.active_search = None;
            }
            if had_filter {
                app.send_action(Action::ResetFilter);
                app.filter.input.clear();
            }

            if had_search && !had_filter {
                app.status_message = Some("Search cleared".to_string());
            }
        }

        // SQL scratchpad
        KeyCode::Char('e') => {
            app.mode = AppMode::Sql;
            // Reset SQL pad state but keep previous SQL text
            app.sql.error = None;
        }

        // Search navigation (n/N always navigate search matches)
        KeyCode::Char('n') => {
            if app.search.active_search.is_some() {
                super::search::jump_to_next_match(app);
            }
        }
        KeyCode::Char('N') => {
            if app.search.active_search.is_some() {
                super::search::jump_to_prev_match(app);
            }
        }

        _ => {}
    }
}
