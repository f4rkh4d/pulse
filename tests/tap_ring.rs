use chrono::Local;
use pulse::tap::{new_ring, TapEvent};

fn mk(i: usize) -> TapEvent {
    TapEvent {
        ts: Local::now(),
        method: "GET".into(),
        path: format!("/{i}"),
        status: Some(200),
        latency_ms: 1,
        req_bytes: 50,
        resp_bytes: 100,
        req_headers: String::new(),
        resp_headers: String::new(),
        req_body_preview: Vec::new(),
        resp_body_preview: Vec::new(),
    }
}

#[test]
fn shared_ring_push_is_threadsafe_shape() {
    let ring = new_ring();
    {
        let mut g = ring.lock().unwrap();
        g.push(mk(1));
        g.push(mk(2));
    }
    let g = ring.lock().unwrap();
    assert_eq!(g.len(), 2);
    assert_eq!(g.get(0).unwrap().path, "/1");
}

#[test]
fn many_pushes_respect_cap() {
    let ring = new_ring();
    let mut g = ring.lock().unwrap();
    for i in 0..600 {
        g.push(mk(i));
    }
    assert_eq!(g.len(), 500);
    // oldest kept is i=100
    assert_eq!(g.get(0).unwrap().path, "/100");
}
