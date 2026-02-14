use crate::app::App;
use crate::event::Action;
use crate::state::SearchMode;

/// Jump to the next cell containing the active search term (full dataset).
pub(crate) fn jump_to_next_match(app: &mut App) {
    if app.tab().search_pending {
        return;
    }
    let term = match app.tab().search.active_search.as_ref() {
        Some(t) => t.clone(),
        None => return,
    };
    let is_regex = app.tab().search.search_mode == SearchMode::Regex;
    app.tab_mut().search_pending = true;
    app.status_message = Some("Searching...".to_string());
    let current_row = app.tab().viewport.selected_row;
    app.send_action(Action::FindMatch {
        term,
        current_row,
        forward: true,
        is_regex,
    });
}

/// Jump to the previous cell containing the active search term (full dataset).
pub(crate) fn jump_to_prev_match(app: &mut App) {
    if app.tab().search_pending {
        return;
    }
    let term = match app.tab().search.active_search.as_ref() {
        Some(t) => t.clone(),
        None => return,
    };
    let is_regex = app.tab().search.search_mode == SearchMode::Regex;
    app.tab_mut().search_pending = true;
    app.status_message = Some("Searching...".to_string());
    let current_row = app.tab().viewport.selected_row;
    app.send_action(Action::FindMatch {
        term,
        current_row,
        forward: false,
        is_regex,
    });
}
