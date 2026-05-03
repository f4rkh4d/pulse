use pulse::config::parse;

#[test]
fn parses_tap_block() {
    let raw = r#"
[[service]]
name = "x"
cmd = "./run"

[service.tap]
mode = "proxy"
listen = 19000
target = 9000
"#;
    let cfg = parse(raw).unwrap();
    let tap = cfg.services[0].tap.as_ref().unwrap();
    assert_eq!(tap.mode, "proxy");
    assert_eq!(tap.listen, Some(19000));
    assert_eq!(tap.target, Some(9000));
}

#[test]
fn tap_defaults_mode_proxy() {
    let raw = r#"
[[service]]
name = "x"
cmd = "./run"

[service.tap]
listen = 19000
"#;
    let cfg = parse(raw).unwrap();
    assert_eq!(cfg.services[0].tap.as_ref().unwrap().mode, "proxy");
}

#[test]
fn global_block_parses() {
    let raw = r#"
[global]
stop_timeout = "3s"
log_buffer = 1000

[[service]]
name = "x"
cmd = "./run"
"#;
    let cfg = parse(raw).unwrap();
    let g = cfg.global.as_ref().unwrap();
    assert_eq!(g.log_buffer, Some(1000));
    assert_eq!(g.stop_timeout_dur(), std::time::Duration::from_secs(3));
}

#[test]
fn missing_global_uses_default_timeout() {
    let raw = r#"
[[service]]
name = "x"
cmd = "./run"
"#;
    let cfg = parse(raw).unwrap();
    assert!(cfg.global.is_none());
}
