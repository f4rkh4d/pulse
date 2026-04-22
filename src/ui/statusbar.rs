use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::theme;
use crate::app::App;

/// fade a color toward the background based on lived / ttl ratio.
fn fade(age_ratio: f32) -> Color {
    // ratio clamped 0..1. at 0 full-fg, at 1 nearly invisible.
    let r = age_ratio.clamp(0.0, 1.0);
    let base = [192u8, 202, 245];
    let bg = [26u8, 27, 38];
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * r) as u8;
    Color::Rgb(
        lerp(base[0], bg[0]),
        lerp(base[1], bg[1]),
        lerp(base[2], bg[2]),
    )
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    // when an agent message is live, show it in place of keybinds
    if let Some(msg) = app.messages.back() {
        let age = msg.born.elapsed().as_secs_f32();
        let ttl = msg.ttl.as_secs_f32().max(0.01);
        let ratio = age / ttl;
        let color = fade(ratio);
        let p = Paragraph::new(Line::from(Span::styled(
            format!("  {}", msg.text),
            Style::default().fg(color),
        )));
        f.render_widget(p, area);
        return;
    }

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
