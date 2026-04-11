use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::theme;
use crate::app::App;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let sep = Span::styled(" │ ", Style::default().fg(theme::BORDER));
    let key = |k: &str, label: &str| -> Vec<Span<'static>> {
        vec![
            Span::styled(k.to_string(), Style::default().fg(theme::FG)),
            Span::styled(format!(" {label}"), Style::default().fg(theme::DIM)),
        ]
    };

    let mut spans: Vec<Span> = Vec::new();
    if app.filter_mode {
        spans.push(Span::styled(
            format!("  filter: /{}_  ", app.filter_input),
            Style::default().fg(theme::FG),
        ));
        spans.push(sep.clone());
        spans.extend(key("enter", "apply"));
        spans.push(sep.clone());
        spans.extend(key("esc", "cancel"));
    } else {
        spans.push(Span::raw("  "));
        spans.extend(key("j/k", "nav"));
        spans.push(sep.clone());
        spans.extend(key("r", "restart"));
        spans.push(sep.clone());
        spans.extend(key("s", "stop"));
        spans.push(sep.clone());
        spans.extend(key("S", "stop all"));
        spans.push(sep.clone());
        spans.extend(key("/", "filter"));
        spans.push(sep.clone());
        spans.extend(key("c", "clear"));
        spans.push(sep.clone());
        spans.extend(key("q", "quit"));
    }

    let p = Paragraph::new(Line::from(spans));
    f.render_widget(p, area);
}
