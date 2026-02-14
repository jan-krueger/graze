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
            let tab = app.tab_mut();
            tab.viewport.selection_mode = match tab.viewport.selection_mode {
                SelectionMode::Row => {
                    tab.viewport.selected_col = tab.viewport.column_offset;
                    SelectionMode::Column
                }
                SelectionMode::Column => SelectionMode::Row,
            };
        }

        // Vertical navigation
        KeyCode::Char('j') | KeyCode::Down => app.move_down(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(1),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let half = app.tab().viewport.page_size / 2;
            app.move_down(half);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let half = app.tab().viewport.page_size / 2;
            app.move_up(half);
        }
        KeyCode::Char('g') => {
            app.tab_mut().viewport.selected_row = 0;
            app.tab_mut().viewport.adjust_view();
            app.ensure_buffer();
        }
        KeyCode::Char('G') => {
            if app.tab().data.total_rows > 0 {
                app.tab_mut().viewport.selected_row = app.tab().data.total_rows - 1;
                app.tab_mut().viewport.adjust_view();
                app.ensure_buffer();
            }
        }

        // Horizontal navigation
        KeyCode::Char('h') | KeyCode::Left => {
            match app.tab().viewport.selection_mode {
                SelectionMode::Row => {
                    if app.tab().viewport.column_offset > 0 {
                        app.tab_mut().viewport.column_offset -= 1;
                    }
                }
                SelectionMode::Column => {
                    if app.tab().viewport.selected_col > 0 {
                        app.tab_mut().viewport.selected_col -= 1;
                        app.adjust_column_view();
                    }
                }
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(ref schema) = app.tab().data.schema {
                let max_col = schema.fields().len().saturating_sub(1);
                match app.tab().viewport.selection_mode {
                    SelectionMode::Row => {
                        if app.tab().viewport.column_offset < max_col {
                            app.tab_mut().viewport.column_offset += 1;
                        }
                    }
                    SelectionMode::Column => {
                        if app.tab().viewport.selected_col < max_col {
                            app.tab_mut().viewport.selected_col += 1;
                            app.adjust_column_view();
                        }
                    }
                }
            }
        }
        KeyCode::Char('0') => {
            match app.tab().viewport.selection_mode {
                SelectionMode::Row => {
                    app.tab_mut().viewport.column_offset = 0;
                }
                SelectionMode::Column => {
                    app.tab_mut().viewport.selected_col = 0;
                    app.adjust_column_view();
                }
            }
        }
        KeyCode::Char('$') => {
            app.tab_mut().filter.input.clear();
            app.mode = AppMode::Regex;
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
                .tab()
                .data
                .table_name
                .as_deref()
                .unwrap_or("data")
                .to_string();
            app.status_message = Some("Loading statistics...".to_string());
            app.send_action(Action::ExecuteSql(format!("SUMMARIZE {table}")));
        }

        // Search / Filter
        KeyCode::Char('/') => {
            app.tab_mut().filter.input.clear();
            app.mode = AppMode::Search;
        }
        KeyCode::Char('f') => {
            // Column-specific filter shortcut: pre-fill with current column name
            if let Some(ref schema) = app.tab().data.schema {
                let fields = schema.fields();
                if !fields.is_empty() {
                    let col_idx = app.tab().viewport.selected_col.min(fields.len() - 1);
                    let col_name = fields[col_idx].name().clone();
                    app.tab_mut().filter.input = format!("\"{}\" = ", col_name);
                    app.mode = AppMode::Filter;
                }
            }
        }
        KeyCode::Esc => {
            let had_search = app.tab().search.active_search.is_some();
            let had_filter = app.tab().filter.active_filter.is_some();

            // Clear both at once
            if had_search {
                let tab = app.tab_mut();
                tab.search.active_search = None;
                tab.search.match_count = None;
                tab.search.match_index = None;
            }
            if had_filter {
                app.send_action(Action::ResetFilter);
                app.tab_mut().filter.input.clear();
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
            if app.tab().search.active_search.is_some() {
                super::search::jump_to_next_match(app);
            }
        }
        KeyCode::Char('N') => {
            if app.tab().search.active_search.is_some() {
                super::search::jump_to_prev_match(app);
            }
        }

        // Tab switching
        KeyCode::Char(']') => {
            if app.has_tabs() {
                app.switch_tab(1);
            }
        }
        KeyCode::Char('[') => {
            if app.has_tabs() {
                app.switch_tab(-1);
            }
        }

        _ => {}
    }
}
