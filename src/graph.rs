//! topological layered layout for the dep graph overlay.
//!
//! nodes without deps land in layer 0. each other node goes into max(dep.layer)+1.
//! within a layer we just preserve config order. not pretty, but predictable.

use std::collections::HashMap;

use crate::config::ServiceSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub name: String,
    pub layer: usize,
    pub col: usize,
}

pub fn layout(services: &[ServiceSpec]) -> Vec<GraphNode> {
    if services.is_empty() {
        return Vec::new();
    }
    let idx: HashMap<&str, usize> = services
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.as_str(), i))
        .collect();
    let order = crate::deps::topo_order(services);
    let mut layer_of: HashMap<String, usize> = HashMap::new();
    for name in &order {
        let i = match idx.get(name.as_str()) {
            Some(&x) => x,
            None => continue,
        };
        let deps = &services[i].depends_on;
        let l = deps
            .iter()
            .filter_map(|d| layer_of.get(d.as_str()).copied())
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        layer_of.insert(name.clone(), l);
    }
    // services not in topo order (orphans from a cycle, but we reject cycles earlier)
    for s in services {
        layer_of.entry(s.name.clone()).or_insert(0);
    }
    // group by layer in config order
    let mut by_layer: Vec<Vec<String>> = Vec::new();
    for s in services {
        let l = layer_of[&s.name];
        while by_layer.len() <= l {
            by_layer.push(Vec::new());
        }
        by_layer[l].push(s.name.clone());
    }
    let mut out = Vec::with_capacity(services.len());
    for (layer, names) in by_layer.iter().enumerate() {
        for (col, name) in names.iter().enumerate() {
            out.push(GraphNode {
                name: name.clone(),
                layer,
                col,
            });
        }
    }
    out
}

pub fn layer_count(nodes: &[GraphNode]) -> usize {
    nodes
        .iter()
        .map(|n| n.layer)
        .max()
        .map(|m| m + 1)
        .unwrap_or(0)
}

pub fn max_cols_per_layer(nodes: &[GraphNode]) -> Vec<usize> {
    let n = layer_count(nodes);
    let mut out = vec![0; n];
    for node in nodes {
        if node.col + 1 > out[node.layer] {
            out[node.layer] = node.col + 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn empty_in_empty_out() {
        assert!(layout(&[]).is_empty());
    }

    #[test]
    fn chain_layers_monotonic() {
        let s = vec![svc("db", &[]), svc("api", &["db"]), svc("web", &["api"])];
        let out = layout(&s);
        let layer_of = |name: &str| out.iter().find(|n| n.name == name).unwrap().layer;
        assert_eq!(layer_of("db"), 0);
        assert_eq!(layer_of("api"), 1);
        assert_eq!(layer_of("web"), 2);
    }

    #[test]
    fn diamond_respects_max_depth() {
        let s = vec![
            svc("root", &[]),
            svc("l", &["root"]),
            svc("r", &["root"]),
            svc("leaf", &["l", "r"]),
        ];
        let out = layout(&s);
        let layer_of = |name: &str| out.iter().find(|n| n.name == name).unwrap().layer;
        assert_eq!(layer_of("root"), 0);
        assert_eq!(layer_of("l"), 1);
        assert_eq!(layer_of("r"), 1);
        assert_eq!(layer_of("leaf"), 2);
    }

    #[test]
    fn all_orphans_sit_on_layer_zero() {
        let s = vec![svc("a", &[]), svc("b", &[]), svc("c", &[])];
        let out = layout(&s);
        assert!(out.iter().all(|n| n.layer == 0));
        // cols run 0, 1, 2
        let mut cols: Vec<usize> = out.iter().map(|n| n.col).collect();
        cols.sort();
        assert_eq!(cols, vec![0, 1, 2]);
    }

    #[test]
    fn layer_count_matches() {
        let s = vec![svc("db", &[]), svc("api", &["db"])];
        let out = layout(&s);
        assert_eq!(layer_count(&out), 2);
    }

    #[test]
    fn max_cols_per_layer_works() {
        let s = vec![svc("a", &[]), svc("b", &[]), svc("c", &["a"])];
        let out = layout(&s);
        let cols = max_cols_per_layer(&out);
        assert_eq!(cols, vec![2, 1]);
    }
}
