use ansi_to_tui::IntoText;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::theme;
use crate::app::App;
use crate::service::Origin;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let svc = app.services.get(app.selected);

    let title = match svc {
        Some(s) => format!(" {} · logs ", s.spec.name),
        None => " logs ".into(),
    };

    let filter_hint = if !app.filter_input.is_empty() || app.filter_mode {
        format!(" /{} ", app.filter_input)
    } else {
        String::new()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            title,
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(filter_hint, Style::default().fg(theme::DIM)));

    let Some(svc) = svc else {
        let empty = Text::from(vec![
            Line::from(""),
            Line::from(Span::styled(
                "   ╱╲  pulse  ╱╲",
                Style::default().fg(theme::DIM),
            )),
            Line::from(Span::styled(
                "   pick a service on the left.",
                Style::default().fg(theme::DIM),
            )),
        ]);
        let p = Paragraph::new(empty)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();
    let name_color = theme::from_name(svc.spec.color.as_deref());

    for log in svc.logs.iter() {
        if let Some(re) = &app.compiled_filter {
            if !re.is_match(&log.text) {
                continue;
            }
        }
        let ts = log.ts.format("%H:%M:%S").to_string();
        let tag = format!("[{}]", svc.spec.name);
        let origin_mark = match log.origin {
            Origin::Stderr => " !",
            Origin::System => " ·",
            Origin::Stdout => "  ",
        };
        // parse ansi escapes from child output, fall back to plain text.
        let body: Text = log
            .text
            .clone()
            .into_text()
            .unwrap_or_else(|_| Text::from(log.text.clone()));
        // each log entry becomes a single line; we splice the ANSI spans in.
        let mut spans: Vec<Span> = vec![
            Span::styled(ts, Style::default().fg(theme::DIM)),
            Span::raw(" "),
            Span::styled(
                tag,
                Style::default().fg(name_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(origin_mark, Style::default().fg(theme::DIM)),
            Span::raw(" "),
        ];
        if let Some(first) = body.lines.into_iter().next() {
            spans.extend(first.spans);
        }
        lines.push(Line::from(spans));
    }

    // tail: only show what fits (let Paragraph handle it, but keep last N)
    let height = area.height.saturating_sub(2) as usize;
    let start = lines.len().saturating_sub(height.max(1));
    let visible = lines[start..].to_vec();

    let p = Paragraph::new(visible)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}
