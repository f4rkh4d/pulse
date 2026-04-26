use pulse::config::{self, parse_duration, ServiceSpec};
use pulse::deps::{dependents_of, find_cycle, topo_order};

fn svc(name: &str, deps: &[&str]) -> ServiceSpec {
    ServiceSpec {
        name: name.into(),
        cmd: "true".into(),
        cwd: None,
        env: Default::default(),
        color: None,
        probe: None,
        port: None,
        agent: None,
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        tap: None,
    }
}

#[test]
fn cycle_two_nodes() {
    let s = vec![svc("a", &["b"]), svc("b", &["a"])];
    assert!(find_cycle(&s).is_some());
}

#[test]
fn cycle_three_nodes() {
    let s = vec![svc("a", &["b"]), svc("b", &["c"]), svc("c", &["a"])];
    assert!(find_cycle(&s).is_some());
}

#[test]
fn no_cycle_diamond() {
    let s = vec![
        svc("root", &[]),
        svc("l", &["root"]),
        svc("r", &["root"]),
        svc("leaf", &["l", "r"]),
    ];
    assert!(find_cycle(&s).is_none());
}

#[test]
fn topo_sort_places_deps_first() {
    let s = vec![svc("web", &["api"]), svc("api", &["db"]), svc("db", &[])];
    let order = topo_order(&s);
    let pos = |n: &str| order.iter().position(|x| x == n).unwrap();
    assert!(pos("db") < pos("api"));
    assert!(pos("api") < pos("web"));
}

#[test]
fn dependents_finds_transitive() {
    let s = vec![svc("db", &[]), svc("api", &["db"]), svc("web", &["api"])];
    let d = dependents_of(&s, "db");
    assert_eq!(d.len(), 2);
}

#[test]
fn config_rejects_cycle() {
    let raw = r#"
        [[service]]
        name = "a"
        cmd = "true"
        depends_on = ["b"]
        [[service]]
        name = "b"
        cmd = "true"
        depends_on = ["a"]
    "#;
    assert!(config::parse(raw).is_err());
}

#[test]
fn duration_parser_basics() {
    assert_eq!(
        parse_duration("250ms").unwrap(),
        std::time::Duration::from_millis(250)
    );
    assert!(parse_duration("crabs").is_none());
}
