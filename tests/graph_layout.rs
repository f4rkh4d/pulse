use pulse::config::ServiceSpec;
use pulse::graph::{layer_count, layout, max_cols_per_layer};

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
        auto_restart: None,
        watch_env: None,
    }
}

#[test]
fn isolated_nodes_share_layer_zero() {
    let s = vec![svc("a", &[]), svc("b", &[])];
    let out = layout(&s);
    assert_eq!(layer_count(&out), 1);
}

#[test]
fn deep_chain_counts_right() {
    let s = vec![
        svc("a", &[]),
        svc("b", &["a"]),
        svc("c", &["b"]),
        svc("d", &["c"]),
    ];
    let out = layout(&s);
    assert_eq!(layer_count(&out), 4);
}

#[test]
fn cols_grow_with_siblings() {
    let s = vec![
        svc("a", &[]),
        svc("b", &["a"]),
        svc("c", &["a"]),
        svc("d", &["a"]),
    ];
    let out = layout(&s);
    let cols = max_cols_per_layer(&out);
    assert_eq!(cols[0], 1);
    assert_eq!(cols[1], 3);
}
