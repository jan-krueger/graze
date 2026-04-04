mod input;
pub(crate) mod mouse;
mod normal;
mod overlay;
pub(crate) mod search;
mod sql;

use crossterm::event::KeyEvent;

use crate::app::App;
use crate::mode::AppMode;

pub(crate) fn handle_key(app: &mut App, key: KeyEvent) {
    match &app.mode {
        AppMode::Normal => normal::handle_key(app, key),
        AppMode::Input(_) => input::handle_key(app, key),
        AppMode::Sql => sql::handle_key(app, key),
        AppMode::Overlay(_) => overlay::handle_key(app, key),
        AppMode::Quitting => {}
    }
}
