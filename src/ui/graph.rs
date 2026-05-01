//! ascii dep graph overlay. reads the layered layout from crate::graph and
//! renders boxes + edges into a ratatui Paragraph.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::theme;
use crate::app::App;
use crate::graph::layout;
use crate::service::Status;

const BOX_W: usize = 18;
const BOX_H: usize = 4;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let specs: Vec<_> = app.services.iter().map(|s| s.spec.clone()).collect();
    let nodes = layout(&specs);
    if nodes.is_empty() {
        let p = Paragraph::new("no services configured")
            .block(block("graph"))
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);
        return;
    }

    let cols_per = crate::graph::max_cols_per_layer(&nodes);
    let layers = cols_per.len();
    let grid_w = cols_per
        .iter()
        .map(|c| c * (BOX_W + 2))
        .max()
        .unwrap_or(BOX_W);
    let grid_h = layers * (BOX_H + 2);

    // pixel (char) canvas
    let mut canvas: Vec<Vec<char>> = vec![vec![' '; grid_w.max(BOX_W)]; grid_h.max(BOX_H)];

    // draw each box
    for node in &nodes {
        let svc = app.services.iter().find(|s| s.spec.name == node.name);
        let (glyph, status_label) = match svc {
            Some(s) => (s.status.dot(), s.status.label()),
            None => ("?", "?"),
        };
        let probe_line = match svc.and_then(|s| s.probe.last_latency) {
            Some(d) => format!("probe {}ms", d.as_millis()),
            None => String::new(),
        };
        let x = node.col * (BOX_W + 2);
        let y = node.layer * (BOX_H + 2);
        draw_box(
            &mut canvas,
            x,
            y,
            BOX_W,
            BOX_H,
            &node.name,
            glyph,
            status_label,
            &probe_line,
        );
    }

    // draw edges: for each service with deps, draw arrow from parent-bottom to child-top
    for (i, spec) in specs.iter().enumerate() {
        let child = &nodes[i];
        for dep in &spec.depends_on {
            let Some(parent) = nodes.iter().find(|n| n.name == *dep) else {
                continue;
            };
            let px = parent.col * (BOX_W + 2) + BOX_W / 2;
            let py = parent.layer * (BOX_H + 2) + BOX_H; // below parent box
            let cx = child.col * (BOX_W + 2) + BOX_W / 2;
            let cy = child.layer * (BOX_H + 2); // top of child
            draw_edge(&mut canvas, px, py, cx, cy);
        }
    }

    // build lines with an accent color for service names and a health color
    let mut lines: Vec<Line> = Vec::with_capacity(canvas.len());
    for row in &canvas {
        let s: String = row.iter().collect();
        lines.push(Line::from(Span::styled(s, Style::default().fg(theme::FG))));
    }

    let health = overall_health(app);
    let color = match health {
        Health::Ok => theme::RUNNING,
        Health::Warn => theme::STARTING,
        Health::Bad => theme::CRASHED,
    };
    let title = Line::from(vec![
        Span::styled(
            " graph ",
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("· {} ", health.label()), Style::default().fg(color)),
    ]);

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color))
                .title(title),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
        ))
}

#[allow(clippy::too_many_arguments)]
fn draw_box(
    c: &mut [Vec<char>],
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    name: &str,
    glyph: &str,
    status: &str,
    extra: &str,
) {
    if y + h >= c.len() || x + w >= c[0].len() {
        return;
    }
    for i in 0..w {
        c[y][x + i] = '─';
        c[y + h - 1][x + i] = '─';
    }
    for j in 0..h {
        c[y + j][x] = '│';
        c[y + j][x + w - 1] = '│';
    }
    c[y][x] = '┌';
    c[y][x + w - 1] = '┐';
    c[y + h - 1][x] = '└';
    c[y + h - 1][x + w - 1] = '┘';

    let label = format!(" {name} ");
    write_str(c, x + 2, y, &label);
    let second = format!(" {} {} ", glyph, status);
    write_str(c, x + 1, y + 1, &second);
    if !extra.is_empty() {
        write_str(c, x + 1, y + 2, extra);
    }
}

fn write_str(c: &mut [Vec<char>], x: usize, y: usize, s: &str) {
    if y >= c.len() {
        return;
    }
    let max = c[y].len();
    for (i, ch) in s.chars().enumerate() {
        if x + i >= max {
            break;
        }
        c[y][x + i] = ch;
    }
}

fn draw_edge(c: &mut [Vec<char>], x1: usize, y1: usize, x2: usize, y2: usize) {
    let mut y = y1;
    let target_y = y2.saturating_sub(1);
    while y < target_y {
        if y < c.len() && x1 < c[y].len() && c[y][x1] == ' ' {
            c[y][x1] = '│';
        }
        y += 1;
    }
    // horizontal segment at the midpoint if x1 != x2
    if x1 != x2 {
        let mid_y = (y1 + y2) / 2;
        let (lo, hi) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
        if mid_y < c.len() {
            for cell in c[mid_y].iter_mut().take(hi + 1).skip(lo) {
                if *cell == ' ' {
                    *cell = '─';
                }
            }
        }
    }
    // arrowhead
    if y2 > 0 && y2 - 1 < c.len() && x2 < c[y2 - 1].len() {
        c[y2 - 1][x2] = '↓';
    }
}

#[derive(Debug, Clone, Copy)]
enum Health {
    Ok,
    Warn,
    Bad,
}
impl Health {
    fn label(self) -> &'static str {
        match self {
            Health::Ok => "all healthy",
            Health::Warn => "some probes failing",
            Health::Bad => "crash present",
        }
    }
}

fn overall_health(app: &App) -> Health {
    let mut worst = Health::Ok;
    for s in &app.services {
        if matches!(s.status, Status::Crashed) {
            return Health::Bad;
        }
        if s.spec.probe.is_some() && s.probe.consecutive_fails > 0 {
            worst = Health::Warn;
        }
    }
    worst
}
