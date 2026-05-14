//! help overlay. one modal listing every keybind, grouped.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::theme;

/// three columns: navigation, actions, views. picked to match the shape
/// of the sidebar so people don't have to re-learn groupings.
pub const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "navigation",
        &[
            ("j / ↓", "next service"),
            ("k / ↑", "prev service"),
            ("enter", "toggle logs focus"),
            ("/", "filter logs"),
        ],
    ),
    (
        "actions",
        &[
            ("r", "restart service"),
            ("x", "stop service"),
            ("S", "stop all"),
            ("c", "clear logs"),
            ("s", "share snapshot now"),
            ("q / ctrl+c", "quit pulse"),
        ],
    ),
    (
        "logs",
        &[
            ("u / pgup", "scroll up a page"),
            ("d / pgdn", "scroll down a page"),
            ("G / end", "jump to tail"),
            ("home / ctrl+g", "jump to top"),
        ],
    ),
    (
        "views",
        &[
            ("t", "tap panel"),
            ("T", "tap request detail"),
            ("g", "dep graph"),
            ("?", "show this help"),
            ("esc", "close overlay"),
        ],
    ),
];

pub fn draw(f: &mut Frame, area: Rect) {
    // height grows with content so it stays legible on small terminals
    let content_h: u16 = SECTIONS
        .iter()
        .map(|(_, b)| b.len() as u16 + 2)
        .sum::<u16>()
        + 4;
    let h = content_h.min(area.height.saturating_sub(2));
    let w: u16 = 64.min(area.width.saturating_sub(2));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(h) / 2),
            Constraint::Length(h),
            Constraint::Min(0),
        ])
        .split(area);
    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(chunks[1].width.saturating_sub(w) / 2),
            Constraint::Length(w),
            Constraint::Min(0),
        ])
        .split(chunks[1]);
    let modal = row[1];

    f.render_widget(Clear, modal);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            "  pulse ",
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(theme::DIM),
        ),
    ]));
    lines.push(Line::from(""));
    for (title, binds) in SECTIONS {
        lines.push(Line::from(Span::styled(
            format!("  {title}"),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in *binds {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    {key:<12}"),
                    Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
                ),
                Span::styled((*desc).to_string(), Style::default().fg(theme::DIM)),
            ]));
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "  press ? or esc to close",
        Style::default()
            .fg(theme::DIM)
            .add_modifier(Modifier::ITALIC),
    )));

    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(Span::styled(
                " help ",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(p, modal);
}

#[cfg(test)]
mod tests {
    use super::SECTIONS;

    #[test]
    fn all_sections_present() {
        let titles: Vec<_> = SECTIONS.iter().map(|(t, _)| *t).collect();
        assert_eq!(titles, vec!["navigation", "actions", "logs", "views"]);
    }

    #[test]
    fn every_section_has_bindings() {
        for (_, b) in SECTIONS {
            assert!(!b.is_empty());
        }
    }

    #[test]
    fn quit_advertised_once() {
        let count = SECTIONS
            .iter()
            .flat_map(|(_, b)| b.iter())
            .filter(|(_, d)| d.contains("quit"))
            .count();
        assert_eq!(count, 1);
    }
}
