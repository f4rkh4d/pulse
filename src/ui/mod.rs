pub mod graph;
pub mod help;
pub mod logs;
pub mod sidebar;
pub mod statusbar;
pub mod tap;
pub mod theme;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::{App, Overlay};

pub fn draw(f: &mut Frame, app: &App) {
    let size = f.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(size);

    match app.overlay {
        Overlay::Graph => {
            graph::draw(f, app, outer[0]);
        }
        Overlay::TapDetail => {
            tap::draw_detail(f, app, outer[0]);
        }
        _ => {
            let body = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(28), Constraint::Min(20)])
                .split(outer[0]);
            sidebar::draw(f, app, body[0]);
            if matches!(app.overlay, Overlay::Tap) {
                tap::draw_panel(f, app, body[1]);
            } else {
                logs::draw(f, app, body[1]);
            }
        }
    }

    statusbar::draw(f, app, outer[1]);

    if matches!(app.overlay, Overlay::Help) {
        help::draw(f, size);
    }
}
