//! actionable error messages for common config/runtime mistakes.
//!
//! goal: if someone mistypes a key or points pulse at a missing cwd, tell
//! them what's wrong and what to try next. generic "ok, something broke"
//! errors waste everyone's time.

use std::path::Path;

use crate::config::ConfigError;

/// wrap a config error with a user-facing suggestion where we have one.
pub fn explain(err: ConfigError, cfg_path: &Path) -> anyhow::Error {
    let suggestion = suggest(&err, cfg_path);
    match suggestion {
        Some(tip) => anyhow::anyhow!("{err}\n  try: {tip}"),
        None => anyhow::anyhow!("{err}"),
    }
}

/// map a ConfigError kind to a short suggestion. None when we have nothing
/// better than the error itself.
pub fn suggest(err: &ConfigError, cfg_path: &Path) -> Option<String> {
    match err {
        ConfigError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => Some(format!(
            "no config at {}. run `pulse init` to draft one",
            cfg_path.display()
        )),
        ConfigError::Io(_) => Some(format!(
            "couldn't read {}. check perms and path",
            cfg_path.display()
        )),
        ConfigError::Toml(_) => {
            Some("toml didn't parse. common causes: missing quotes, tab indent, stray `=`".into())
        }
        ConfigError::Empty => {
            Some("no services. add at least one `[[service]]` block with `name` + `cmd`".into())
        }
        ConfigError::EmptyCmd(_) => Some("`cmd` is empty. put the shell invocation there".into()),
        ConfigError::BadCmd(_) => {
            Some("unterminated quote in `cmd`. balance your \"...\" or '...' pairs".into())
        }
        ConfigError::Duplicate(name) => Some(format!(
            "two services named `{name}`. rename one or merge them"
        )),
        ConfigError::BadDuration { val, .. } => Some(format!(
            "`{val}` isn't a duration. use `500ms`, `2s`, `1m`, or `1h`"
        )),
        ConfigError::UnknownDep { dep, .. } => Some(format!(
            "nothing named `{dep}`. check for typos in `depends_on`"
        )),
        ConfigError::SelfDep { svc } => Some(format!("drop `{svc}` from its own `depends_on`")),
        ConfigError::Cycle(n) => Some(format!(
            "cycle through `{n}`. break it by removing one edge in `depends_on`"
        )),
        ConfigError::BadAgent { kind, .. } => Some(format!(
            "`{kind}` isn't a species. pick one of: goblin, cat, ghost, robot, blob"
        )),
    }
}

/// hint for a runtime port-bind failure. called from the supervisor path
/// when a child dies immediately with a port-in-use symptom.
pub fn port_in_use_hint(port: u16) -> String {
    format!("port {port} is already bound. try `pulse ports` to see who owns it, then kill or pick a different port")
}

/// hint when a cwd doesn't exist before spawn. we check this up front so the
/// child doesn't spiral into rapid-restart.
pub fn missing_cwd_hint(cwd: &Path) -> String {
    format!(
        "cwd `{}` doesn't exist. fix the path in `pulse.toml` or `mkdir -p` first",
        cwd.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pp() -> PathBuf {
        PathBuf::from("pulse.toml")
    }

    #[test]
    fn suggests_on_missing_file() {
        let err = ConfigError::Io(std::io::Error::from(std::io::ErrorKind::NotFound));
        let s = suggest(&err, &pp()).unwrap();
        assert!(s.contains("pulse init"));
    }

    #[test]
    fn suggests_on_bad_duration() {
        let err = ConfigError::BadDuration {
            svc: "x".into(),
            val: "soon".into(),
        };
        let s = suggest(&err, &pp()).unwrap();
        assert!(s.contains("500ms"));
    }

    #[test]
    fn suggests_on_bad_agent() {
        let err = ConfigError::BadAgent {
            svc: "x".into(),
            kind: "dragon".into(),
        };
        let s = suggest(&err, &pp()).unwrap();
        assert!(s.contains("goblin"));
    }

    #[test]
    fn suggests_on_unknown_dep() {
        let err = ConfigError::UnknownDep {
            svc: "x".into(),
            dep: "nope".into(),
        };
        let s = suggest(&err, &pp()).unwrap();
        assert!(s.contains("typos"));
    }

    #[test]
    fn suggests_on_cycle() {
        let s = suggest(&ConfigError::Cycle("a".into()), &pp()).unwrap();
        assert!(s.contains("cycle") || s.contains("edge"));
    }

    #[test]
    fn port_hint_references_pulse_ports() {
        assert!(port_in_use_hint(3000).contains("pulse ports"));
        assert!(port_in_use_hint(3000).contains("3000"));
    }

    #[test]
    fn cwd_hint_mentions_path() {
        let p = PathBuf::from("./nope");
        assert!(missing_cwd_hint(&p).contains("nope"));
    }

    #[test]
    fn explain_prepends_suggestion() {
        let err = ConfigError::BadDuration {
            svc: "x".into(),
            val: "q".into(),
        };
        let wrapped = explain(err, &pp());
        let msg = format!("{wrapped}");
        assert!(msg.contains("try:"));
    }
}
