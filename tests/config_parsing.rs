use pulse::config::{parse, ServiceSpec};
use std::io::Write;

#[test]
fn reads_example_file() {
    let raw = std::fs::read_to_string("examples/pulse.toml").expect("example exists");
    let cfg = parse(&raw).expect("parse");
    assert_eq!(cfg.services.len(), 4);
    assert_eq!(cfg.services[0].name, "api");
    assert_eq!(cfg.services[3].name, "redis");
}

#[test]
fn roundtrip_from_tempfile() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        f,
        r#"
[[service]]
name = "x"
cmd = "echo ok"
"#
    )
    .unwrap();
    let raw = std::fs::read_to_string(f.path()).unwrap();
    let cfg = parse(&raw).unwrap();
    assert_eq!(cfg.services[0].parse_cmd().unwrap(), vec!["echo", "ok"]);
}

#[test]
fn rejects_empty_cmd() {
    let raw = r#"
[[service]]
name = "x"
cmd = ""
"#;
    assert!(parse(raw).is_err());
}

#[test]
fn env_roundtrips() {
    let raw = r#"
[[service]]
name = "x"
cmd = "./run"
env = { A = "1", B = "two" }
"#;
    let cfg = parse(raw).unwrap();
    assert_eq!(cfg.services[0].env.get("A").unwrap(), "1");
    assert_eq!(cfg.services[0].env.get("B").unwrap(), "two");
}

#[test]
fn spec_parse_cmd_quotes() {
    let spec = ServiceSpec {
        name: "x".into(),
        cmd: "sh -c 'echo hi there'".into(),
        cwd: None,
        env: Default::default(),
        color: None,
        probe: None,
        port: None,
        agent: None,
        depends_on: Vec::new(),
        tap: None,
    };
    assert_eq!(spec.parse_cmd().unwrap(), vec!["sh", "-c", "echo hi there"]);
}
