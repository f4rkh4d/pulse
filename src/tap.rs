//! tiny reverse-proxy tap. listens on one localhost port, forwards raw bytes
//! to another localhost port, and records every round-trip's framing.
//!
//! only parses enough of http/1.1 to pull method + path + status. everything
//! else is copied verbatim. connection: keep-alive / websocket upgrades fall
//! through untouched because after the first parse we stop inspecting and
//! just proxy bytes til either side hangs up.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub const RING_CAP: usize = 500;
const HEADER_MAX: usize = 16 * 1024;
const BODY_PREVIEW_MAX: usize = 4096;

#[derive(Debug, Clone)]
pub struct TapEvent {
    pub ts: DateTime<Local>,
    pub method: String,
    pub path: String,
    pub status: Option<u16>,
    pub latency_ms: u128,
    pub req_bytes: usize,
    pub resp_bytes: usize,
    pub req_headers: String,
    pub resp_headers: String,
    pub req_body_preview: Vec<u8>,
    pub resp_body_preview: Vec<u8>,
}

#[derive(Debug, Default, Clone)]
pub struct TapRing {
    inner: VecDeque<TapEvent>,
}

impl TapRing {
    pub fn push(&mut self, ev: TapEvent) {
        if self.inner.len() >= RING_CAP {
            self.inner.pop_front();
        }
        self.inner.push_back(ev);
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &TapEvent> {
        self.inner.iter()
    }
    pub fn get(&self, i: usize) -> Option<&TapEvent> {
        self.inner.get(i)
    }
    pub fn cap() -> usize {
        RING_CAP
    }
}

pub type SharedRing = Arc<Mutex<TapRing>>;

pub fn new_ring() -> SharedRing {
    Arc::new(Mutex::new(TapRing::default()))
}

/// mode from config; only "proxy" is implemented for real.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Proxy,
    Passive,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s {
            "passive" => Mode::Passive,
            _ => Mode::Proxy,
        }
    }
}

/// try to derive the target port from ServiceSpec tap/port/probe fields.
pub fn derive_target(
    tap: &crate::config::TapSpec,
    port: Option<u16>,
    probe_url: Option<&str>,
) -> Option<u16> {
    if let Some(t) = tap.target {
        return Some(t);
    }
    if let Some(p) = port {
        return Some(p);
    }
    // try to pull :port out of a probe url like http://127.0.0.1:3000/health
    if let Some(url) = probe_url {
        if let Some(rest) = url.split_once("://").map(|(_, r)| r) {
            let host_port = rest.split('/').next().unwrap_or("");
            if let Some((_host, p)) = host_port.rsplit_once(':') {
                if let Ok(n) = p.parse::<u16>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// spawn a proxy listener. drops quietly if bind fails. returns the bound
/// port (may differ from requested if caller passed 0) so callers can log it.
pub async fn run_proxy(listen: u16, target: u16, ring: SharedRing) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", listen)).await?;
    let bound = listener.local_addr()?.port();
    tokio::spawn(async move {
        loop {
            let (client, _addr) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => continue,
            };
            let ring = ring.clone();
            tokio::spawn(async move {
                let _ = handle_conn(client, target, ring).await;
            });
        }
    });
    Ok(bound)
}

async fn handle_conn(mut client: TcpStream, target: u16, ring: SharedRing) -> std::io::Result<()> {
    // read until end-of-headers or buffer full. then dial upstream, forward
    // the raw bytes we've read, then bi-directional copy until one side closes,
    // inspecting the upstream-to-client stream's first chunk for a status line.
    let mut req_buf = Vec::with_capacity(2048);
    let header_end = match read_until_headers(&mut client, &mut req_buf).await {
        Some(n) => n,
        None => return Ok(()),
    };
    let (method, path, req_headers_str) = parse_request_head(&req_buf[..header_end]);

    let start = Instant::now();
    let mut upstream = match TcpStream::connect(("127.0.0.1", target)).await {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    // forward what we read
    upstream.write_all(&req_buf).await?;

    // now read the response head from upstream so we can log status.
    let mut resp_buf = Vec::with_capacity(2048);
    let resp_head_end = read_until_headers(&mut upstream, &mut resp_buf).await;

    // forward response head to client
    if !resp_buf.is_empty() {
        client.write_all(&resp_buf).await?;
    }

    let (status, resp_headers_str) = match resp_head_end {
        Some(n) => parse_response_head(&resp_buf[..n]),
        None => (None, String::new()),
    };

    // capture body previews before we pour bytes past
    let req_body_preview = slice_body_preview(&req_buf, header_end);
    let resp_body_preview = match resp_head_end {
        Some(n) => slice_body_preview(&resp_buf, n),
        None => Vec::new(),
    };

    // now pipe the rest both ways
    let (req_extra, resp_extra) = bidi_copy(client, upstream).await;

    let ev = TapEvent {
        ts: Local::now(),
        method,
        path,
        status,
        latency_ms: start.elapsed().as_millis(),
        req_bytes: req_buf.len() + req_extra,
        resp_bytes: resp_buf.len() + resp_extra,
        req_headers: req_headers_str,
        resp_headers: resp_headers_str,
        req_body_preview,
        resp_body_preview,
    };
    if let Ok(mut g) = ring.lock() {
        g.push(ev);
    }
    Ok(())
}

async fn read_until_headers(sock: &mut TcpStream, buf: &mut Vec<u8>) -> Option<usize> {
    let mut tmp = [0u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if buf.len() > HEADER_MAX {
            return None;
        }
        if Instant::now() >= deadline {
            return None;
        }
        let n = match tokio::time::timeout(Duration::from_secs(30), sock.read(&mut tmp)).await {
            Ok(Ok(0)) => return None,
            Ok(Ok(n)) => n,
            _ => return None,
        };
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_double_crlf(buf) {
            return Some(pos + 4);
        }
    }
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn parse_request_head(head: &[u8]) -> (String, String, String) {
    let text = String::from_utf8_lossy(head).to_string();
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    (method, path, text)
}

fn parse_response_head(head: &[u8]) -> (Option<u16>, String) {
    let text = String::from_utf8_lossy(head).to_string();
    let status = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok());
    (status, text)
}

fn slice_body_preview(buf: &[u8], head_end: usize) -> Vec<u8> {
    let body = buf.get(head_end..).unwrap_or(&[]);
    let take = body.len().min(BODY_PREVIEW_MAX);
    body[..take].to_vec()
}

async fn bidi_copy(mut a: TcpStream, mut b: TcpStream) -> (usize, usize) {
    let (mut ar, mut aw) = a.split();
    let (mut br, mut bw) = b.split();
    let f1 = tokio::io::copy(&mut ar, &mut bw);
    let f2 = tokio::io::copy(&mut br, &mut aw);
    let (r1, r2) = tokio::join!(f1, f2);
    (r1.unwrap_or(0) as usize, r2.unwrap_or(0) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_caps_at_500() {
        let mut r = TapRing::default();
        for i in 0..(RING_CAP + 10) {
            r.push(TapEvent {
                ts: Local::now(),
                method: "GET".into(),
                path: format!("/{i}"),
                status: Some(200),
                latency_ms: 1,
                req_bytes: 0,
                resp_bytes: 0,
                req_headers: String::new(),
                resp_headers: String::new(),
                req_body_preview: Vec::new(),
                resp_body_preview: Vec::new(),
            });
        }
        assert_eq!(r.len(), RING_CAP);
        // oldest kept is event #10
        assert_eq!(r.get(0).unwrap().path, "/10");
    }

    #[test]
    fn ring_starts_empty() {
        let r = TapRing::default();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn parses_request_head() {
        let raw = b"GET /foo HTTP/1.1\r\nHost: x\r\n\r\n";
        let (m, p, h) = parse_request_head(raw);
        assert_eq!(m, "GET");
        assert_eq!(p, "/foo");
        assert!(h.contains("Host: x"));
    }

    #[test]
    fn parses_response_status() {
        let raw = b"HTTP/1.1 204 No Content\r\nServer: x\r\n\r\n";
        let (s, _) = parse_response_head(raw);
        assert_eq!(s, Some(204));
    }

    #[test]
    fn finds_body_preview() {
        let mut buf = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nhi!".to_vec();
        let end = find_double_crlf(&buf).unwrap() + 4;
        let prev = slice_body_preview(&buf, end);
        assert_eq!(prev, b"hi!");
        // also for headers without body
        buf.truncate(end);
        let empty = slice_body_preview(&buf, end);
        assert!(empty.is_empty());
    }

    #[test]
    fn mode_parses() {
        assert_eq!(Mode::parse("proxy"), Mode::Proxy);
        assert_eq!(Mode::parse("passive"), Mode::Passive);
        assert_eq!(Mode::parse(""), Mode::Proxy);
    }

    #[test]
    fn derives_target_from_probe_url() {
        let tap = crate::config::TapSpec {
            mode: "proxy".into(),
            listen: Some(13000),
            target: None,
        };
        let got = derive_target(&tap, None, Some("http://127.0.0.1:3000/health"));
        assert_eq!(got, Some(3000));
    }

    #[test]
    fn derives_target_from_port_expect() {
        let tap = crate::config::TapSpec {
            mode: "proxy".into(),
            listen: Some(13000),
            target: None,
        };
        assert_eq!(derive_target(&tap, Some(8080), None), Some(8080));
    }

    #[test]
    fn explicit_target_wins() {
        let tap = crate::config::TapSpec {
            mode: "proxy".into(),
            listen: Some(13000),
            target: Some(9999),
        };
        assert_eq!(derive_target(&tap, Some(3000), None), Some(9999));
    }
}
