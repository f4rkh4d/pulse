//! live tap panel — scrollable list of captured http round-trips.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::theme;
use crate::app::App;

pub fn draw_panel(f: &mut Frame, app: &App, area: Rect) {
    let svc = app.services.get(app.selected);
    let name = svc.map(|s| s.spec.name.as_str()).unwrap_or("");
    let ring = app.tap_rings.get(app.selected).and_then(|r| r.as_ref());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            format!(" {name} · tap "),
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        ));

    let Some(r) = ring else {
        let p = Paragraph::new(Line::from(Span::styled(
            "  no [service.tap] configured for this service.",
            Style::default().fg(theme::DIM),
        )))
        .block(block);
        f.render_widget(p, area);
        return;
    };

    let events: Vec<_> = {
        let g = match r.lock() {
            Ok(g) => g,
            Err(_) => {
                f.render_widget(block, area);
                return;
            }
        };
        g.iter().cloned().collect()
    };

    if events.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled(
            "  waiting for requests...",
            Style::default().fg(theme::DIM),
        )))
        .block(block);
        f.render_widget(p, area);
        return;
    }

    let height = area.height.saturating_sub(2) as usize;
    let start = events.len().saturating_sub(height.max(1));
    let mut lines = Vec::with_capacity(events.len() - start);
    for ev in &events[start..] {
        let color = match ev.status {
            Some(s) if (200..300).contains(&s) => theme::RUNNING,
            Some(s) if (400..600).contains(&s) => theme::CRASHED,
            Some(_) => theme::STARTING,
            None => theme::DIM,
        };
        lines.push(Line::from(vec![
            Span::styled(
                ev.ts.format("%H:%M:%S").to_string(),
                Style::default().fg(theme::DIM),
            ),
            Span::raw(" "),
            Span::styled(format!("{:<5}", ev.method), Style::default().fg(theme::FG)),
            Span::styled(
                truncate(&ev.path, 48),
                Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                ev.status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "---".into()),
                Style::default().fg(color),
            ),
            Span::styled(
                format!("  {}ms  {}→{}B", ev.latency_ms, ev.req_bytes, ev.resp_bytes),
                Style::default().fg(theme::DIM),
            ),
        ]));
    }

    let p = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

pub fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let ring = app.tap_rings.get(app.selected).and_then(|r| r.as_ref());
    let Some(r) = ring else {
        return;
    };
    let ev = {
        let g = match r.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match g.iter().last().cloned() {
            Some(e) => e,
            None => return,
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let req_title = format!(" {} {} ", ev.method, truncate(&ev.path, 40));
    let req_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            req_title,
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        ));
    let req_body = format!(
        "{}\n\n{}",
        ev.req_headers,
        String::from_utf8_lossy(&ev.req_body_preview)
    );
    f.render_widget(
        Paragraph::new(req_body)
            .block(req_block)
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let status = ev
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "---".into());
    let resp_title = format!(" ← {} · {}ms ", status, ev.latency_ms);
    let resp_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            resp_title,
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        ));
    let resp_body = format!(
        "{}\n\n{}",
        ev.resp_headers,
        String::from_utf8_lossy(&ev.resp_body_preview)
    );
    f.render_widget(
        Paragraph::new(resp_body)
            .block(resp_block)
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        format!("{s:<n$}", n = n)
    } else {
        let taken: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{taken}…")
    }
}
