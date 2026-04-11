pub mod logs;
pub mod sidebar;
pub mod statusbar;
pub mod theme;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let size = f.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(size);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(20)])
        .split(outer[0]);

    sidebar::draw(f, app, body[0]);
    logs::draw(f, app, body[1]);
    statusbar::draw(f, app, outer[1]);
}
