//! dependency graph. used for cycle detection at config-load time and for
//! figuring out which services to restart when a dep flips.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::config::ServiceSpec;

/// returns the name of any node that sits on a cycle, or None if the graph is acyclic.
pub fn find_cycle(services: &[ServiceSpec]) -> Option<String> {
    let idx: HashMap<&str, usize> = services
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.as_str(), i))
        .collect();
    let n = services.len();
    let mut color = vec![0u8; n]; // 0 unvisited, 1 on-stack, 2 done
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for start in 0..n {
        if color[start] != 0 {
            continue;
        }
        stack.push((start, 0));
        color[start] = 1;
        while let Some(&(node, dep_i)) = stack.last() {
            let deps = &services[node].depends_on;
            if dep_i >= deps.len() {
                color[node] = 2;
                stack.pop();
                continue;
            }
            stack.last_mut().unwrap().1 += 1;
            let dep_name = &deps[dep_i];
            let next = match idx.get(dep_name.as_str()) {
                Some(&i) => i,
                None => continue,
            };
            match color[next] {
                0 => {
                    color[next] = 1;
                    stack.push((next, 0));
                }
                1 => return Some(services[next].name.clone()),
                _ => {}
            }
        }
    }
    None
}

/// kahn's topo sort. returns names in start-order (deps first). assumes acyclic.
pub fn topo_order(services: &[ServiceSpec]) -> Vec<String> {
    let idx: HashMap<&str, usize> = services
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.as_str(), i))
        .collect();
    let n = services.len();
    let mut indeg = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, s) in services.iter().enumerate() {
        for dep in &s.depends_on {
            if let Some(&j) = idx.get(dep.as_str()) {
                adj[j].push(i);
                indeg[i] += 1;
            }
        }
    }
    let mut q: VecDeque<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut out = Vec::with_capacity(n);
    while let Some(u) = q.pop_front() {
        out.push(services[u].name.clone());
        for &v in &adj[u] {
            indeg[v] -= 1;
            if indeg[v] == 0 {
                q.push_back(v);
            }
        }
    }
    out
}

/// return indices of all services that directly or transitively depend on `name`.
pub fn dependents_of(services: &[ServiceSpec], name: &str) -> Vec<usize> {
    let mut out: HashSet<usize> = HashSet::new();
    let mut frontier: Vec<&str> = vec![name];
    while let Some(cur) = frontier.pop() {
        for (i, s) in services.iter().enumerate() {
            if s.depends_on.iter().any(|d| d == cur) && out.insert(i) {
                frontier.push(s.name.as_str());
            }
        }
    }
    let mut v: Vec<usize> = out.into_iter().collect();
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceSpec;

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
    fn detects_simple_cycle() {
        let s = vec![svc("a", &["b"]), svc("b", &["a"])];
        assert!(find_cycle(&s).is_some());
    }

    #[test]
    fn no_cycle_in_chain() {
        let s = vec![svc("a", &[]), svc("b", &["a"]), svc("c", &["b"])];
        assert!(find_cycle(&s).is_none());
    }

    #[test]
    fn topo_respects_order() {
        let s = vec![svc("web", &["api"]), svc("api", &["db"]), svc("db", &[])];
        let order = topo_order(&s);
        let pos = |n: &str| order.iter().position(|x| x == n).unwrap();
        assert!(pos("db") < pos("api"));
        assert!(pos("api") < pos("web"));
    }

    #[test]
    fn dependents_transitive() {
        let s = vec![
            svc("db", &[]),
            svc("api", &["db"]),
            svc("web", &["api"]),
            svc("ops", &[]),
        ];
        let d = dependents_of(&s, "db");
        // api (1) and web (2) depend on db, ops (3) does not
        assert!(d.contains(&1));
        assert!(d.contains(&2));
        assert!(!d.contains(&3));
    }
}
