use crate::app::App;

/// Jump to the next match using the cached match_rows list (instant).
pub(crate) fn jump_to_next_match(app: &mut App) {
    let current_row = app.tab().viewport.selected_row;
    let result = app.tab().search.next_match(current_row);
    if let Some((idx, row)) = result {
        app.tab_mut().search.match_index = Some(idx + 1);
        app.tab_mut().viewport.selected_row = row;
        app.tab_mut().viewport.adjust_view();
        app.ensure_buffer();
        app.status_message = Some(format!("Match at row {}", row + 1));
    } else {
        app.status_message = Some("No match found".to_string());
    }
}

/// Jump to the previous match using the cached match_rows list (instant).
pub(crate) fn jump_to_prev_match(app: &mut App) {
    let current_row = app.tab().viewport.selected_row;
    let result = app.tab().search.prev_match(current_row);
    if let Some((idx, row)) = result {
        app.tab_mut().search.match_index = Some(idx + 1);
        app.tab_mut().viewport.selected_row = row;
        app.tab_mut().viewport.adjust_view();
        app.ensure_buffer();
        app.status_message = Some(format!("Match at row {}", row + 1));
    } else {
        app.status_message = Some("No match found".to_string());
    }
}
