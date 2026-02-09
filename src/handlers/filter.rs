use crossterm::event::KeyCode;

use crate::app::{App, AppMode};
use crate::event::Action;
use crate::state::is_filter_expression;

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            app.filter.autocomplete_active = false;
            if !app.filter.input.is_empty() {
                if is_filter_expression(&app.filter.input) {
                    // Filter path: expand $refs and send SQL filter
                    let expanded = expand_dollar_refs(&app.filter.input);
                    app.filter.input = expanded.clone();
                    app.send_action(Action::Filter(expanded));
                } else {
                    // Search path: plain text substring search
                    app.search.active_search = Some(app.filter.input.clone());
                }
            }
            app.mode = AppMode::Normal;
        }
        KeyCode::Esc => {
            if app.filter.autocomplete_active {
                app.filter.autocomplete_active = false;
            } else {
                app.filter.input.clear();
                app.mode = AppMode::Normal;
            }
        }
        KeyCode::Tab => {
            if app.filter.autocomplete_active
                && !app.filter.autocomplete_suggestions.is_empty()
            {
                let idx = app
                    .filter
                    .autocomplete_index
                    .min(app.filter.autocomplete_suggestions.len() - 1);
                let chosen = app.filter.autocomplete_suggestions[idx].clone();
                apply_autocomplete(app, &chosen);
                app.filter.autocomplete_active = false;
                app.filter.autocomplete_suggestions.clear();
            }
        }
        KeyCode::Up => {
            if app.filter.autocomplete_active
                && !app.filter.autocomplete_suggestions.is_empty()
            {
                if app.filter.autocomplete_index == 0 {
                    app.filter.autocomplete_index =
                        app.filter.autocomplete_suggestions.len() - 1;
                } else {
                    app.filter.autocomplete_index -= 1;
                }
            }
        }
        KeyCode::Down => {
            if app.filter.autocomplete_active
                && !app.filter.autocomplete_suggestions.is_empty()
            {
                app.filter.autocomplete_index = (app.filter.autocomplete_index + 1)
                    % app.filter.autocomplete_suggestions.len();
            }
        }
        KeyCode::Backspace => {
            app.filter.input.pop();
            update_autocomplete(app);
        }
        KeyCode::Char(c) => {
            app.filter.input.push(c);
            update_autocomplete(app);
        }
        _ => {}
    }
}

fn update_autocomplete(app: &mut App) {
    let (partial, _start) = match find_dollar_prefix(&app.filter.input) {
        Some(v) => v,
        None => {
            app.filter.autocomplete_active = false;
            app.filter.autocomplete_suggestions.clear();
            return;
        }
    };

    let schema = match app.data.schema.as_ref() {
        Some(s) => s,
        None => {
            app.filter.autocomplete_active = false;
            return;
        }
    };

    let partial_lower = partial.to_lowercase();
    let suggestions: Vec<String> = schema
        .fields()
        .iter()
        .filter(|f| f.name().to_lowercase().starts_with(&partial_lower))
        .map(|f| f.name().clone())
        .collect();

    if suggestions.is_empty() {
        app.filter.autocomplete_active = false;
        app.filter.autocomplete_suggestions.clear();
    } else {
        app.filter.autocomplete_active = true;
        app.filter.autocomplete_suggestions = suggestions;
        if app.filter.autocomplete_index >= app.filter.autocomplete_suggestions.len() {
            app.filter.autocomplete_index = 0;
        }
    }
}

fn apply_autocomplete(app: &mut App, name: &str) {
    let (_partial, start) = match find_dollar_prefix(&app.filter.input) {
        Some(v) => v,
        None => return,
    };

    app.filter.input.truncate(start);

    let needs_quoting = name.contains(' ') || name.contains('"');
    if needs_quoting {
        app.filter.input.push('$');
        app.filter.input.push('"');
        app.filter.input.push_str(name);
        app.filter.input.push('"');
    } else {
        app.filter.input.push('$');
        app.filter.input.push_str(name);
    }
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
