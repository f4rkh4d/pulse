//! help overlay. one modal listing every keybind, grouped.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::theme;

pub fn draw(f: &mut Frame, area: Rect) {
    // center a 60x20 modal inside area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(22) / 2),
            Constraint::Length(22),
            Constraint::Min(0),
        ])
        .split(area);
    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(chunks[1].width.saturating_sub(60) / 2),
            Constraint::Length(60.min(chunks[1].width)),
            Constraint::Min(0),
        ])
        .split(chunks[1]);
    let modal = row[1];

    f.render_widget(Clear, modal);

    let sections: &[(&str, &[(&str, &str)])] = &[
        (
            "nav",
            &[
                ("j / ↓", "next service"),
                ("k / ↑", "prev service"),
                ("enter", "toggle logs focus"),
            ],
        ),
        (
            "lifecycle",
            &[
                ("r", "restart"),
                ("s", "stop"),
                ("S", "stop all"),
                ("c", "clear logs"),
            ],
        ),
        (
            "views",
            &[
                ("t", "tap panel"),
                ("T", "tap request detail"),
                ("g", "dep graph"),
                ("/", "filter logs"),
                ("?", "help (this)"),
                ("esc", "close overlay"),
            ],
        ),
        ("quit", &[("q", "quit"), ("ctrl+c", "quit")]),
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " pulse keybinds ",
        Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    for (title, binds) in sections {
        lines.push(Line::from(Span::styled(
            format!(" {title}"),
            Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in *binds {
            lines.push(Line::from(vec![
                Span::styled(format!("   {key:<10}"), Style::default().fg(theme::FG)),
                Span::styled((*desc).to_string(), Style::default().fg(theme::DIM)),
            ]));
        }
        lines.push(Line::from(""));
    }

    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(" ? "),
    );
    f.render_widget(p, modal);
}
