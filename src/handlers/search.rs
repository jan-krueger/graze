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
        let wrapped = if row <= current_row { " (wrapped)" } else { "" };
        app.status_message = Some(format!("Match at row {}{wrapped}", row + 1));
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
        let wrapped = if row >= current_row { " (wrapped)" } else { "" };
        app.status_message = Some(format!("Match at row {}{wrapped}", row + 1));
    } else {
        app.status_message = Some("No match found".to_string());
    }
}

/// Jump to the next marked row after the current position (wraps around).
pub(crate) fn jump_to_next_mark(app: &mut App) {
    let marks = &app.tab().marked_rows;
    if marks.is_empty() {
        app.status_message = Some("No marks".to_string());
        return;
    }
    let mut sorted: Vec<usize> = marks.iter().copied().collect();
    sorted.sort_unstable();
    let total = sorted.len();
    let current_row = app.tab().viewport.selected_row;

    // Find first mark > current_row
    let pos = sorted.partition_point(|&r| r <= current_row);
    let (idx, wrapped) = if pos < sorted.len() {
        (pos, false)
    } else {
        (0, true) // wrap to start
    };
    let row = sorted[idx];
    app.tab_mut().viewport.selected_row = row;
    app.tab_mut().viewport.adjust_view();
    app.ensure_buffer();
    let wrap_str = if wrapped { " (wrapped)" } else { "" };
    app.status_message = Some(format!("Mark {}/{total}{wrap_str}", idx + 1));
}

/// Jump to the previous marked row before the current position (wraps around).
pub(crate) fn jump_to_prev_mark(app: &mut App) {
    let marks = &app.tab().marked_rows;
    if marks.is_empty() {
        app.status_message = Some("No marks".to_string());
        return;
    }
    let mut sorted: Vec<usize> = marks.iter().copied().collect();
    sorted.sort_unstable();
    let total = sorted.len();
    let current_row = app.tab().viewport.selected_row;

    // Find last mark < current_row
    let pos = sorted.partition_point(|&r| r < current_row);
    let (idx, wrapped) = if pos > 0 {
        (pos - 1, false)
    } else {
        (sorted.len() - 1, true) // wrap to end
    };
    let row = sorted[idx];
    app.tab_mut().viewport.selected_row = row;
    app.tab_mut().viewport.adjust_view();
    app.ensure_buffer();
    let wrap_str = if wrapped { " (wrapped)" } else { "" };
    app.status_message = Some(format!("Mark {}/{total}{wrap_str}", idx + 1));
}
