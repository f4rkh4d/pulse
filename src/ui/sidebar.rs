use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;

use super::theme;
use crate::app::App;
use crate::service::Status;

fn status_color(s: Status) -> ratatui::style::Color {
    match s {
        Status::Running => theme::RUNNING,
        Status::Starting => theme::STARTING,
        Status::Crashed => theme::CRASHED,
        Status::Stopped => theme::STOPPED,
    }
}

fn fmt_uptime(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .services
        .iter()
        .map(|svc| {
            let dot = Span::styled(
                format!(" {} ", svc.status.dot()),
                Style::default().fg(status_color(svc.status)),
            );
            let name = Span::styled(
                svc.spec.name.clone(),
                Style::default()
                    .fg(theme::from_name(svc.spec.color.as_deref()))
                    .add_modifier(Modifier::BOLD),
            );
            let uptime = match svc.uptime() {
                Some(d) if svc.status == Status::Running => {
                    format!("  {}", fmt_uptime(d))
                }
                _ => String::new(),
            };
            let up_span = Span::styled(uptime, Style::default().fg(theme::DIM));
            ListItem::new(Line::from(vec![dot, name, up_span]))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            " pulse ",
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        ));

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(theme::BG_SEL)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut state);
}
