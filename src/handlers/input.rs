use crossterm::event::KeyCode;

use crate::app::App;
use crate::event::Action;
use crate::mode::{AppMode, InputVariant};
pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    let variant = match &app.mode {
        AppMode::Input(v) => *v,
        _ => return,
    };

    match key.code {
        KeyCode::Enter => on_enter(app, variant),
        KeyCode::Esc => on_escape(app, variant),
        KeyCode::Tab => on_tab(app, variant),
        KeyCode::Up => on_up(app, variant),
        KeyCode::Down => on_down(app, variant),
        KeyCode::Backspace => {
            app.text_input.delete_before_cursor();
            if variant == InputVariant::Filter {
                update_autocomplete(app);
            }
        }
        KeyCode::Char(c) => {
            app.text_input.insert_char(c);
            if variant == InputVariant::Filter {
                update_autocomplete(app);
            }
        }
        KeyCode::Left if !app.autocomplete.active => {
            app.text_input.move_cursor_left();
        }
        KeyCode::Right if !app.autocomplete.active => {
            app.text_input.move_cursor_right();
        }
        KeyCode::Home => app.text_input.move_cursor_to_start(),
        KeyCode::End => app.text_input.move_cursor_to_end(),
        _ => {}
    }
}

fn on_enter(app: &mut App, variant: InputVariant) {
    app.autocomplete.clear();

    if !app.text_input.is_empty() {
        match variant {
            InputVariant::Filter => {
                let expanded = expand_dollar_refs(&app.text_input.input);
                app.text_input.set(&expanded);
                let filter_text = app.text_input.input.clone();
                app.filter_history.push(filter_text.clone());
                // Store active_filter now since text_input will be cleared
                app.tab_mut().filter.active_filter = Some(filter_text.clone());
                app.send_action(Action::Filter(filter_text));
            }
            InputVariant::Search => {
                let input = app.text_input.input.clone();
                let tab = app.tab_mut();
                tab.search.active_search = Some(input.clone());
                tab.search.match_count = None;
                tab.search.match_index = None;
                tab.search.match_rows.clear();
                tab.search_pending = true;
                app.send_action(Action::CollectMatches(input));
            }
            InputVariant::GoToRow => {
                let input = app.text_input.input.clone();
                if let Ok(row_num) = input.parse::<usize>() {
                    let total = app.tab().data.total_rows;
                    if row_num >= 1 && (total == 0 || row_num <= total) {
                        app.tab_mut().viewport.selected_row = row_num - 1;
                        app.tab_mut().viewport.adjust_view();
                        app.ensure_buffer();
                        app.status_message = Some(format!("Jumped to row {row_num}"));
                    } else {
                        app.status_message = Some(format!("Row must be between 1 and {total}"));
                        return; // Don't exit mode on invalid input
                    }
                } else {
                    app.status_message = Some("Invalid row number".to_string());
                    return; // Don't exit mode on invalid input
                }
            }
        }
    } else if variant == InputVariant::GoToRow {
        app.status_message = Some("Enter a row number".to_string());
        return;
    }

    app.transition_to(AppMode::Normal);
}

fn on_escape(app: &mut App, _variant: InputVariant) {
    if app.autocomplete.active {
        app.autocomplete.clear();
    } else {
        app.transition_to(AppMode::Normal);
    }
}

fn on_tab(app: &mut App, variant: InputVariant) {
    if variant == InputVariant::Filter
        && app.autocomplete.active
        && !app.autocomplete.suggestions.is_empty()
    {
        let chosen = match app.autocomplete.selected() {
            Some(s) => s.to_string(),
            None => return,
        };
        apply_autocomplete(app, &chosen);
        app.autocomplete.clear();
    }
}

fn on_up(app: &mut App, variant: InputVariant) {
    if variant == InputVariant::Filter && app.autocomplete.active {
        app.autocomplete.move_up();
    } else if variant == InputVariant::Filter && !app.autocomplete.active {
        // Filter history recall
        if let Some(entry) = app.filter_history.up() {
            app.text_input.set(entry);
        }
    }
}

fn on_down(app: &mut App, variant: InputVariant) {
    if variant == InputVariant::Filter && app.autocomplete.active {
        app.autocomplete.move_down();
    } else if variant == InputVariant::Filter && !app.autocomplete.active {
        // Filter history recall
        if let Some(entry) = app.filter_history.down() {
            app.text_input.set(entry);
        } else {
            app.text_input.clear();
        }
    }
}

fn update_autocomplete(app: &mut App) {
    let (partial, _start) = match find_dollar_prefix(&app.text_input.input) {
        Some(v) => v,
        None => {
            app.autocomplete.clear();
            return;
        }
    };

    let schema = match app.tab().data.schema.as_ref() {
        Some(s) => s.clone(),
        None => {
            app.autocomplete.clear();
            return;
        }
    };

    let partial_lower = partial.to_lowercase();
    let field_names_lower = &app.tab().data.field_names_lower;
    let suggestions: Vec<String> = schema
        .fields()
        .iter()
        .zip(field_names_lower.iter())
        .filter(|(_, lower)| lower.starts_with(&partial_lower))
        .map(|(f, _)| f.name().clone())
        .collect();

    app.autocomplete.set_suggestions(suggestions);
}

fn apply_autocomplete(app: &mut App, name: &str) {
    let (_partial, start) = match find_dollar_prefix(&app.text_input.input) {
        Some(v) => v,
        None => return,
    };

    app.text_input.input.truncate(start);

    let needs_quoting = name.contains(' ') || name.contains('"');
    if needs_quoting {
        app.text_input.input.push('$');
        app.text_input.input.push('"');
        app.text_input.input.push_str(name);
        app.text_input.input.push('"');
    } else {
        app.text_input.input.push('$');
        app.text_input.input.push_str(name);
    }
    app.text_input.move_cursor_to_end();
}

fn find_dollar_prefix(input: &str) -> Option<(String, usize)> {
    let dollar_pos = input.rfind('$')?;
    let after_dollar = &input[dollar_pos + 1..];

    if after_dollar.is_empty() {
        return Some((String::new(), dollar_pos));
    }

    if let Some(rest) = after_dollar.strip_prefix('"') {
        if rest.contains('"') {
            return None;
        }
        return Some((rest.to_string(), dollar_pos));
    }

    let mut chars = after_dollar.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        Some(_) => return None,
        None => return Some((String::new(), dollar_pos)),
    }

    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
    }

    Some((after_dollar.to_string(), dollar_pos))
}

pub fn expand_dollar_refs(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'"' {
                if let Some(close) = input[i + 2..].find('"') {
                    let name = &input[i + 2..i + 2 + close];
                    result.push('"');
                    result.push_str(name);
                    result.push('"');
                    i = i + 2 + close + 1;
                    continue;
                } else {
                    let name = &input[i + 2..];
                    result.push('"');
                    result.push_str(name);
                    result.push('"');
                    break;
                }
            }

            let start = i + 1;
            let first = bytes[start];
            if first.is_ascii_alphabetic() || first == b'_' {
                let mut end = start + 1;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                let name = &input[start..end];
                result.push('"');
                result.push_str(name);
                result.push('"');
                i = end;
                continue;
            }
        }

        result.push(bytes[i] as char);
        i += 1;
    }

    result
}
