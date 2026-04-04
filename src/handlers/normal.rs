use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::App;
use crate::event::{Action, SortState};
use crate::mode::{AppMode, InputVariant, OverlayVariant};
use crate::state::{SearchState, SelectionMode, next_visible_col};

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
        KeyCode::PageDown => {
            let page = app.tab().viewport.page_size;
            app.move_down(page);
        }
        KeyCode::PageUp => {
            let page = app.tab().viewport.page_size;
            app.move_up(page);
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
                    let min_offset = app.tab().viewport.frozen_cols;
                    if app.tab().viewport.column_offset > min_offset {
                        let hidden = &app.tab().hidden_columns;
                        let max_col = app.tab().data.schema.as_ref()
                            .map_or(0, |s| s.fields().len().saturating_sub(1));
                        let new_off = next_visible_col(app.tab().viewport.column_offset, -1, hidden, max_col);
                        app.tab_mut().viewport.column_offset = new_off.max(min_offset);
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
        // Sort
        KeyCode::Char('s') => app.cycle_sort(),

        // Reset view
        KeyCode::Char('r') => {
            let tab = app.tab_mut();
            tab.search = SearchState::new();
            tab.filter.active_filter = None;
            tab.data.sort_state.clear();
            tab.hidden_columns.fill(false);
            tab.viewport.view_start = 0;
            tab.viewport.selected_row = 0;
            tab.viewport.column_offset = 0;
            tab.viewport.selected_col = 0;
            tab.col_width_cache.replace(None);
            app.send_action(Action::ResetFilter);
            app.send_action(Action::ApplySort(SortState::default()));
            app.status_message = Some("View reset".into());
            app.ensure_buffer();
        }

        // Stats overlay
        KeyCode::Char('S') => {
            app.transition_to(AppMode::Overlay(OverlayVariant::Stats));
        }

        // Column jump (fuzzy find)
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(ref schema) = app.tab().data.schema {
                let columns: Vec<(usize, String)> = schema
                    .fields()
                    .iter()
                    .enumerate()
                    .map(|(i, f)| (i, f.name().clone()))
                    .collect();
                app.column_jump.populate(columns);
                app.transition_to(AppMode::Overlay(OverlayVariant::ColumnJump));
            }
        }

        // Search / Filter
        KeyCode::Char('/') => {
            app.text_input.clear();
            app.mode = AppMode::Input(InputVariant::Search);
        }
        KeyCode::Char('f') => {
            // Column-specific filter shortcut: pre-fill with current column name
            if let Some(ref schema) = app.tab().data.schema {
                let fields = schema.fields();
                if !fields.is_empty() {
                    let col_idx = app.tab().viewport.selected_col.min(fields.len() - 1);
                    let col_name = fields[col_idx].name().clone();
                    app.text_input.set(&format!("\"{}\" = ", col_name));
                    app.mode = AppMode::Input(InputVariant::Filter);
                }
            }
        }
        KeyCode::Esc => {
            let has_search = app.tab().search.active_search.is_some();
            let has_filter = app.tab().filter.active_filter.is_some();

            if has_search {
                // First Esc: clear search
                let tab = app.tab_mut();
                tab.search.active_search = None;
                tab.search.match_count = None;
                tab.search.match_index = None;
                tab.search.match_rows.clear();
                app.status_message = Some("Search cleared".to_string());
            } else if has_filter {
                // Second Esc: clear filter
                app.send_action(Action::ResetFilter);
                app.tab_mut().filter.active_filter = None;
            }
        }

        // SQL scratchpad
        KeyCode::Char('e') => {
            app.transition_to(AppMode::Sql);
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

        // Go-to-row
        KeyCode::Char(':') => {
            app.text_input.clear();
            app.text_input.char_filter = Some(|c: char| c.is_ascii_digit());
            app.mode = AppMode::Input(InputVariant::GoToRow);
        }

        // Mark/unmark row
        KeyCode::Char('m') => {
            let row = app.tab().viewport.selected_row;
            let was_marked = app.tab().marked_rows.contains(&row);
            if was_marked {
                app.tab_mut().marked_rows.remove(&row);
            } else {
                app.tab_mut().marked_rows.insert(row);
            }
            let total = app.tab().marked_rows.len();
            if was_marked {
                app.status_message = Some(format!("Unmarked row {} ({total} marked)", row + 1));
            } else {
                app.status_message = Some(format!("Marked row {} ({total} marked)", row + 1));
            }
        }
        // Clear all marks
        KeyCode::Char('M') => {
            let count = app.tab().marked_rows.len();
            if count > 0 {
                app.tab_mut().marked_rows.clear();
                app.status_message = Some(format!("Cleared {count} mark(s)"));
            } else {
                app.status_message = Some("No marks to clear".into());
            }
        }

        // Mark navigation
        KeyCode::Char('}') => {
            super::search::jump_to_next_mark(app);
        }
        KeyCode::Char('{') => {
            super::search::jump_to_prev_mark(app);
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
                app.column_picker.populate(columns, selected);
                app.mode = AppMode::Overlay(OverlayVariant::ColumnPicker);
            }
        }

        // Freeze/unfreeze columns
        KeyCode::Char('F') => {
            let frozen = app.tab().viewport.frozen_cols;
            let offset = app.tab().viewport.column_offset;
            if frozen > 0 {
                app.tab_mut().viewport.frozen_cols = 0;
                app.status_message = Some("Columns unfrozen".into());
            } else if offset > 0 {
                app.tab_mut().viewport.frozen_cols = offset;
                app.status_message = Some(format!("Froze {} column(s)", offset));
            } else {
                app.status_message = Some("Scroll right first, then freeze".into());
            }
        }

        // Toggle line wrapping
        KeyCode::Char('W') => {
            app.wrap = !app.wrap;
            app.tab_mut().col_width_cache.replace(None);
            app.status_message = Some(if app.wrap { "Wrap: on" } else { "Wrap: off" }.into());
        }

        // Help
        KeyCode::Char('?') => {
            app.mode = AppMode::Overlay(OverlayVariant::Help);
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
