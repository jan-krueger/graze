use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, AppMode};
use crate::event::Action;
use crate::state::{SelectionMode, next_visible_col};

const COL_WIDTH_STEP: i16 = 2;
const MAX_COL_WIDTH_OVERRIDE: i16 = 450;
const MIN_COL_WIDTH_OVERRIDE: i16 = -46;

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
            let total = app.tab().data.total_rows;
            if total > 0 {
                app.tab_mut().viewport.selected_row = total - 1;
                app.tab_mut().viewport.adjust_view();
                app.ensure_buffer();
            }
        }

        // Horizontal navigation
        KeyCode::Char('h') | KeyCode::Left => {
            match app.tab().viewport.selection_mode {
                SelectionMode::Row => {
                    if app.tab().viewport.column_offset > 0 {
                        let hidden = &app.tab().hidden_columns;
                        let max_col = app.tab().data.schema.as_ref()
                            .map_or(0, |s| s.fields().len().saturating_sub(1));
                        let new_off = next_visible_col(app.tab().viewport.column_offset, -1, hidden, max_col);
                        app.tab_mut().viewport.column_offset = new_off;
                    }
                }
                SelectionMode::Column => {
                    let hidden = &app.tab().hidden_columns;
                    let cur = app.tab().viewport.selected_col;
                    if let Some(ref schema) = app.tab().data.schema {
                        let max_col = schema.fields().len().saturating_sub(1);
                        let new_col = next_visible_col(cur, -1, hidden, max_col);
                        app.tab_mut().viewport.selected_col = new_col;
                        app.adjust_column_view();
                    }
                }
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(ref schema) = app.tab().data.schema {
                let max_col = schema.fields().len().saturating_sub(1);
                let hidden = &app.tab().hidden_columns;
                match app.tab().viewport.selection_mode {
                    SelectionMode::Row => {
                        let new_off = next_visible_col(app.tab().viewport.column_offset, 1, hidden, max_col);
                        app.tab_mut().viewport.column_offset = new_off;
                    }
                    SelectionMode::Column => {
                        let cur = app.tab().viewport.selected_col;
                        let new_col = next_visible_col(cur, 1, hidden, max_col);
                        app.tab_mut().viewport.selected_col = new_col;
                        app.adjust_column_view();
                    }
                }
            }
        }
        KeyCode::Char('0') => {
            let hidden = &app.tab().hidden_columns;
            let first = crate::state::first_visible_col(hidden,
                app.tab().data.schema.as_ref().map_or(0, |s| s.fields().len().saturating_sub(1)));
            match app.tab().viewport.selection_mode {
                SelectionMode::Row => {
                    app.tab_mut().viewport.column_offset = first;
                }
                SelectionMode::Column => {
                    app.tab_mut().viewport.selected_col = first;
                    app.adjust_column_view();
                }
            }
        }
        KeyCode::Char('$') => {
            let filter = &mut app.tab_mut().filter;
            filter.input.clear();
            filter.cursor_pos = 0;
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
            let filter = &mut app.tab_mut().filter;
            filter.input.clear();
            filter.cursor_pos = 0;
            app.mode = AppMode::Search;
        }
        KeyCode::Char('f') => {
            // Column-specific filter shortcut: pre-fill with current column name
            if let Some(ref schema) = app.tab().data.schema {
                let fields = schema.fields();
                if !fields.is_empty() {
                    let col_idx = app.tab().viewport.selected_col.min(fields.len() - 1);
                    let col_name = fields[col_idx].name().clone();
                    let filter = &mut app.tab_mut().filter;
                    filter.input = format!("\"{}\" = ", col_name);
                    filter.move_cursor_to_end();
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
                tab.search.match_rows.clear();
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

        // Diff mode
        KeyCode::Char('D') => {
            app.enter_diff_setup();
        }

        // Go-to-row
        KeyCode::Char(':') => {
            let filter = &mut app.tab_mut().filter;
            filter.input.clear();
            filter.cursor_pos = 0;
            app.mode = AppMode::GoToRow;
        }

        // Column hide picker
        KeyCode::Char('H') => {
            if let Some(ref schema) = app.tab().data.schema {
                let columns: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
                let selected = if app.tab().hidden_columns.len() == columns.len() {
                    app.tab().hidden_columns.clone()
                } else {
                    vec![false; columns.len()]
                };
                app.diff.setup.columns = columns;
                app.diff.setup.selected = selected;
                app.diff.setup.cursor = 0;
                app.mode = AppMode::ColumnHide;
            }
        }

        // Per-column width adjust
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let col = app.tab().viewport.selected_col;
            let overrides = &mut app.tab_mut().viewport.col_width_overrides;
            if col < overrides.len() {
                overrides[col] = (overrides[col] + COL_WIDTH_STEP).min(MAX_COL_WIDTH_OVERRIDE);
            }
        }
        KeyCode::Char('-') => {
            let col = app.tab().viewport.selected_col;
            let overrides = &mut app.tab_mut().viewport.col_width_overrides;
            if col < overrides.len() {
                overrides[col] = (overrides[col] - COL_WIDTH_STEP).max(MIN_COL_WIDTH_OVERRIDE);
            }
        }

        // Help
        KeyCode::Char('?') => {
            app.mode = AppMode::Help;
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
