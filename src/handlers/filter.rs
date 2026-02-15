use crossterm::event::KeyCode;

use crate::app::{App, AppMode};
use crate::event::Action;
use crate::state::SearchMode;

pub(crate) fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            app.tab_mut().filter.autocomplete_active = false;
            if !app.tab().filter.input.is_empty() {
                match app.mode {
                    AppMode::Filter => {
                        let expanded = expand_dollar_refs(&app.tab().filter.input);
                        app.tab_mut().filter.input = expanded.clone();
                        app.send_action(Action::Filter(expanded));
                    }
                    AppMode::Regex => {
                        app.tab_mut().search.search_mode = SearchMode::Regex;
                        let input = app.tab().filter.input.clone();
                        let tab = app.tab_mut();
                        tab.search.active_search = Some(input.clone());
                        tab.search.match_count = None;
                        tab.search.match_index = None;
                        app.send_action(Action::CountMatches {
                            term: input,
                            is_regex: true,
                        });
                    }
                    AppMode::Search => {
                        app.tab_mut().search.search_mode = SearchMode::Plain;
                        let input = app.tab().filter.input.clone();
                        let tab = app.tab_mut();
                        tab.search.active_search = Some(input.clone());
                        tab.search.match_count = None;
                        tab.search.match_index = None;
                        app.send_action(Action::CountMatches {
                            term: input,
                            is_regex: false,
                        });
                    }
                    _ => unreachable!(),
                }
            }
            app.mode = AppMode::Normal;
        }
        KeyCode::Esc => {
            if app.tab().filter.autocomplete_active {
                app.tab_mut().filter.autocomplete_active = false;
            } else {
                app.tab_mut().filter.input.clear();
                app.mode = AppMode::Normal;
            }
        }
        KeyCode::Tab => {
            if app.mode == AppMode::Filter
                && app.tab().filter.autocomplete_active
                && !app.tab().filter.autocomplete_suggestions.is_empty()
            {
                let idx = app
                    .tab()
                    .filter
                    .autocomplete_index
                    .min(app.tab().filter.autocomplete_suggestions.len() - 1);
                let chosen = app.tab().filter.autocomplete_suggestions[idx].clone();
                apply_autocomplete(app, &chosen);
                let tab = app.tab_mut();
                tab.filter.autocomplete_active = false;
                tab.filter.autocomplete_suggestions.clear();
            }
        }
        KeyCode::Up => {
            if app.mode == AppMode::Filter
                && app.tab().filter.autocomplete_active
                && !app.tab().filter.autocomplete_suggestions.is_empty()
            {
                let tab = app.tab_mut();
                if tab.filter.autocomplete_index == 0 {
                    tab.filter.autocomplete_index =
                        tab.filter.autocomplete_suggestions.len() - 1;
                } else {
                    tab.filter.autocomplete_index -= 1;
                }
            }
        }
        KeyCode::Down => {
            if app.mode == AppMode::Filter
                && app.tab().filter.autocomplete_active
                && !app.tab().filter.autocomplete_suggestions.is_empty()
            {
                let tab = app.tab_mut();
                tab.filter.autocomplete_index = (tab.filter.autocomplete_index + 1)
                    % tab.filter.autocomplete_suggestions.len();
            }
        }
        KeyCode::Backspace => {
            app.tab_mut().filter.delete_before_cursor();
            if app.mode == AppMode::Filter {
                update_autocomplete(app);
            }
        }
        KeyCode::Char(c) => {
            app.tab_mut().filter.insert_at_cursor(c);
            if app.mode == AppMode::Filter {
                update_autocomplete(app);
            }
        }
        KeyCode::Left if !app.tab().filter.autocomplete_active => {
            app.tab_mut().filter.move_cursor_left();
        }
        KeyCode::Right if !app.tab().filter.autocomplete_active => {
            app.tab_mut().filter.move_cursor_right();
        }
        KeyCode::Home => app.tab_mut().filter.move_cursor_to_start(),
        KeyCode::End => app.tab_mut().filter.move_cursor_to_end(),
        _ => {}
    }
}

fn update_autocomplete(app: &mut App) {
    let (partial, _start) = match find_dollar_prefix(&app.tab().filter.input) {
        Some(v) => v,
        None => {
            let tab = app.tab_mut();
            tab.filter.autocomplete_active = false;
            tab.filter.autocomplete_suggestions.clear();
            return;
        }
    };

    let schema = match app.tab().data.schema.as_ref() {
        Some(s) => s.clone(),
        None => {
            app.tab_mut().filter.autocomplete_active = false;
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

    let tab = app.tab_mut();
    if suggestions.is_empty() {
        tab.filter.autocomplete_active = false;
        tab.filter.autocomplete_suggestions.clear();
    } else {
        tab.filter.autocomplete_active = true;
        tab.filter.autocomplete_suggestions = suggestions;
        if tab.filter.autocomplete_index >= tab.filter.autocomplete_suggestions.len() {
            tab.filter.autocomplete_index = 0;
        }
    }
}

fn apply_autocomplete(app: &mut App, name: &str) {
    let (_partial, start) = match find_dollar_prefix(&app.tab().filter.input) {
        Some(v) => v,
        None => return,
    };

    let tab = app.tab_mut();
    tab.filter.input.truncate(start);

    let needs_quoting = name.contains(' ') || name.contains('"');
    if needs_quoting {
        tab.filter.input.push('$');
        tab.filter.input.push('"');
        tab.filter.input.push_str(name);
        tab.filter.input.push('"');
    } else {
        tab.filter.input.push('$');
        tab.filter.input.push_str(name);
    }
    tab.filter.move_cursor_to_end();
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
