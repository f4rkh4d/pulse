//! snapshot a stack's state to a self-contained html file. no CDN, no fonts.

use std::time::Duration;

use chrono::Local;
use html_escape::encode_text;

use crate::service::{Service, Status};
use crate::tap::TapEvent;

pub struct ServiceSnapshot<'a> {
    pub name: &'a str,
    pub status: Status,
    pub uptime: Option<Duration>,
    pub restart_count: u32,
    pub probe_ok_rate: Option<f32>,
    pub probe_last_status: Option<u16>,
    pub probe_last_ms: Option<u128>,
    pub tap: Vec<&'a TapEvent>,
}

pub fn collect<'a>(
    services: &'a [Service],
    tap_rings: &'a [Option<crate::tap::SharedRing>],
) -> Vec<ServiceSnapshot<'a>> {
    services
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let tap = if let Some(r) = tap_rings.get(i).and_then(|r| r.as_ref()) {
                let guard = r.lock().expect("tap ring poisoned");
                let all: Vec<TapEvent> = guard.iter().cloned().collect();
                drop(guard);
                // leak into a Box so references stay valid for render pass
                let boxed: &'a [TapEvent] = Box::leak(all.into_boxed_slice());
                let take = boxed.len().saturating_sub(50);
                boxed[take..].iter().collect()
            } else {
                Vec::new()
            };
            ServiceSnapshot {
                name: &s.spec.name,
                status: s.status,
                uptime: s.uptime(),
                restart_count: s.restart_count,
                probe_ok_rate: s.probe.success_rate(),
                probe_last_status: s.probe.last_status,
                probe_last_ms: s.probe.last_latency.map(|d| d.as_millis()),
                tap,
            }
        })
        .collect()
}

pub fn render(snap: &[ServiceSnapshot<'_>]) -> String {
    let mut body = String::new();
    body.push_str(&format!(
        "<h1>pulse snapshot</h1><p class=\"sub\">captured {}</p>",
        Local::now().format("%Y-%m-%d %H:%M:%S %z")
    ));
    body.push_str("<table class=\"summary\"><thead><tr><th>service</th><th>status</th><th>uptime</th><th>restarts</th><th>probe</th></tr></thead><tbody>");
    for s in snap {
        let uptime = s.uptime.map(fmt_dur).unwrap_or_else(|| "—".into());
        let probe = match (s.probe_last_status, s.probe_last_ms, s.probe_ok_rate) {
            (Some(code), Some(ms), Some(rate)) => {
                format!("{} · {}ms · {:.0}%", code, ms, rate * 100.0)
            }
            _ => "—".into(),
        };
        body.push_str(&format!(
            "<tr><td class=\"name\">{}</td><td class=\"st-{}\">{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            encode_text(s.name),
            s.status.label(),
            s.status.label(),
            uptime,
            s.restart_count,
            encode_text(&probe),
        ));
    }
    body.push_str("</tbody></table>");

    for s in snap {
        if s.tap.is_empty() {
            continue;
        }
        body.push_str(&format!(
            "<h2>{} · last {} tap events</h2>",
            encode_text(s.name),
            s.tap.len()
        ));
        body.push_str("<table class=\"tap\"><thead><tr><th>time</th><th>method</th><th>path</th><th>status</th><th>ms</th><th>bytes in/out</th></tr></thead><tbody>");
        for ev in &s.tap {
            body.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}/{}</td></tr>",
                ev.ts.format("%H:%M:%S"),
                encode_text(&ev.method),
                encode_text(&ev.path),
                ev.status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "—".into()),
                ev.latency_ms,
                ev.req_bytes,
                ev.resp_bytes,
            ));
        }
        body.push_str("</tbody></table>");
    }

    body.push_str("<footer>exported from pulse · github.com/f4rkh4d/pulse</footer>");

    format!(
        "<!doctype html><html lang=en><head><meta charset=utf-8><title>pulse snapshot</title><style>{}</style></head><body>{}</body></html>",
        CSS, body
    )
}

fn fmt_dur(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

const CSS: &str = r#"
body { background:#0f1116; color:#c0caf5; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; max-width: 960px; margin: 2rem auto; padding: 0 1.5rem; line-height: 1.5; }
h1 { color:#5eead4; margin: 0 0 0.2rem; font-size: 1.4rem; }
h2 { color:#5eead4; margin: 2rem 0 0.5rem; font-size: 1.1rem; }
.sub { color:#828bac; margin: 0 0 1.5rem; font-size: 0.85rem; }
table { width: 100%; border-collapse: collapse; margin: 0.5rem 0 1rem; font-size: 0.9rem; }
th, td { text-align: left; padding: 0.35rem 0.6rem; border-bottom: 1px solid #252839; }
th { color:#828bac; font-weight: normal; }
.name { color:#5eead4; }
.st-running { color:#9ece6a; }
.st-crashed { color:#f7768e; }
.st-starting { color:#e0af68; }
.st-stopped { color:#828bac; }
footer { color:#565f89; font-size: 0.8rem; margin-top: 3rem; text-align: center; }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn mk_snap() -> Vec<ServiceSnapshot<'static>> {
        vec![ServiceSnapshot {
            name: "api",
            status: Status::Running,
            uptime: Some(Duration::from_secs(3725)),
            restart_count: 2,
            probe_ok_rate: Some(0.98),
            probe_last_status: Some(200),
            probe_last_ms: Some(42),
            tap: Vec::new(),
        }]
    }

    #[test]
    fn renders_summary_row() {
        let s = mk_snap();
        let html = render(&s);
        assert!(html.contains("api"));
        assert!(html.contains("running"));
        assert!(html.contains("01:02:05"));
        assert!(html.contains("200 · 42ms · 98%"));
    }

    #[test]
    fn escapes_service_name() {
        let tap: Vec<&TapEvent> = Vec::new();
        let snap = vec![ServiceSnapshot {
            name: "<script>",
            status: Status::Stopped,
            uptime: None,
            restart_count: 0,
            probe_ok_rate: None,
            probe_last_status: None,
            probe_last_ms: None,
            tap,
        }];
        let html = render(&snap);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn includes_footer_attribution() {
        let html = render(&mk_snap());
        assert!(html.contains("github.com/f4rkh4d/pulse"));
    }

    #[test]
    fn tap_section_renders_when_present() {
        let ev = TapEvent {
            ts: Local::now(),
            method: "GET".into(),
            path: "/v1/things".into(),
            status: Some(204),
            latency_ms: 7,
            req_bytes: 90,
            resp_bytes: 12,
            req_headers: String::new(),
            resp_headers: String::new(),
            req_body_preview: Vec::new(),
            resp_body_preview: Vec::new(),
        };
        let leaked: &'static TapEvent = Box::leak(Box::new(ev));
        let snap = vec![ServiceSnapshot {
            name: "api",
            status: Status::Running,
            uptime: None,
            restart_count: 0,
            probe_ok_rate: None,
            probe_last_status: None,
            probe_last_ms: None,
            tap: vec![leaked],
        }];
        let html = render(&snap);
        assert!(html.contains("/v1/things"));
        assert!(html.contains("204"));
    }

    #[test]
    fn self_contained_no_external_urls() {
        let html = render(&mk_snap());
        assert!(!html.contains("http://"));
        // github.com is inside a text footer without protocol
        assert!(!html.contains("https://"));
        assert!(!html.contains("<link"));
        assert!(!html.contains("<script"));
    }
}
