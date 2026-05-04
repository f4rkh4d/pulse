use pulse::service::Status;
use pulse::share::{render, ServiceSnapshot};
use std::time::Duration;

fn snap() -> Vec<ServiceSnapshot<'static>> {
    vec![
        ServiceSnapshot {
            name: "api",
            status: Status::Running,
            uptime: Some(Duration::from_secs(90)),
            restart_count: 0,
            probe_ok_rate: Some(1.0),
            probe_last_status: Some(200),
            probe_last_ms: Some(8),
            tap: Vec::new(),
        },
        ServiceSnapshot {
            name: "db",
            status: Status::Crashed,
            uptime: None,
            restart_count: 4,
            probe_ok_rate: None,
            probe_last_status: None,
            probe_last_ms: None,
            tap: Vec::new(),
        },
    ]
}

#[test]
fn both_services_render() {
    let html = render(&snap());
    assert!(html.contains("api"));
    assert!(html.contains("db"));
}

#[test]
fn status_classes_applied() {
    let html = render(&snap());
    assert!(html.contains("st-running"));
    assert!(html.contains("st-crashed"));
}

#[test]
fn embeds_css_inline() {
    let html = render(&snap());
    assert!(html.contains("<style>"));
    assert!(!html.contains("<link "));
}

#[test]
fn footer_attribution_present() {
    let html = render(&snap());
    assert!(html.contains("exported from pulse"));
}
