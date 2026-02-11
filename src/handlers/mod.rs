mod filter;
mod normal;
mod search;
mod sql;
mod stats;

use crossterm::event::KeyEvent;

use crate::app::{App, AppMode};

pub(crate) fn handle_key(app: &mut App, key: KeyEvent) {
    match app.mode {
        AppMode::Normal => normal::handle_key(app, key),
        AppMode::Filter | AppMode::Search | AppMode::Regex => filter::handle_key(app, key),
        AppMode::Sql => sql::handle_key(app, key),
        AppMode::Stats => stats::handle_key(app, key),
        AppMode::Quitting => {}
    }
}
