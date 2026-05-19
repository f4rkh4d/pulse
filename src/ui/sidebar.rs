use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;

use super::theme;
use crate::app::App;
use crate::service::{Service, Status};

fn status_color(s: Status) -> ratatui::style::Color {
    match s {
        Status::Running => theme::RUNNING,
        Status::Starting => theme::STARTING,
        Status::Crashed => theme::CRASHED,
        Status::CrashedTooMany => theme::CRASHED,
        Status::Stopped => theme::STOPPED,
    }
}

fn probe_color(svc: &Service) -> ratatui::style::Color {
    let rate = svc.probe.success_rate().unwrap_or(0.0);
    if svc.probe.healthy() && rate > 0.9 {
        theme::RUNNING
    } else if rate > 0.6 {
        theme::STARTING
    } else {
        theme::CRASHED
    }
}

fn fmt_uptime(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn fmt_latency(d: std::time::Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f32 / 1000.0)
    }
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
            let face = match &svc.agent {
                Some(a) => Span::styled(format!("{} ", a.face()), Style::default().fg(theme::DIM)),
                None => Span::raw(""),
            };
            let name = Span::styled(
                svc.spec.name.clone(),
                Style::default()
                    .fg(theme::from_name(svc.spec.color.as_deref()))
                    .add_modifier(Modifier::BOLD),
            );
            let unhealthy = if svc.unhealthy {
                Span::styled(" •", Style::default().fg(theme::CRASHED))
            } else {
                Span::raw("")
            };
            let too_many = if matches!(svc.status, Status::CrashedTooMany) {
                Span::styled("  [crashed-too-many]", Style::default().fg(theme::CRASHED))
            } else {
                Span::raw("")
            };
            let uptime = match svc.uptime() {
                Some(d) if svc.status == Status::Running => {
                    format!("  {}", fmt_uptime(d))
                }
                _ => String::new(),
            };
            let up_span = Span::styled(uptime, Style::default().fg(theme::DIM));

            let line1: Vec<Span> = vec![dot, face, name, unhealthy, up_span, too_many];
            let mut lines = vec![Line::from(line1)];

            // probe badge line
            if svc.spec.probe.is_some() {
                let badge = match (svc.probe.last_status, svc.probe.last_latency) {
                    (Some(code), Some(lat)) => {
                        let rate = svc.probe.success_rate().unwrap_or(0.0) * 100.0;
                        format!("   {} · {} · {:.0}%", code, fmt_latency(lat), rate)
                    }
                    _ => "   probing...".into(),
                };
                lines.push(Line::from(Span::styled(
                    badge,
                    Style::default().fg(probe_color(svc)),
                )));
            }
            // port badge line
            if let Some(pp) = &svc.spec.port {
                let (txt, color) = match svc.port.last_bound {
                    Some(true) => (format!("   :{} bound", pp.expect), theme::RUNNING),
                    Some(false) => (format!("   :{} free", pp.expect), theme::DIM),
                    None => (format!("   :{} ?", pp.expect), theme::DIM),
                };
                lines.push(Line::from(Span::styled(txt, Style::default().fg(color))));
            }

            ListItem::new(lines)
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
