//! `pulse shell <service>` — drop into a subshell configured like the service.
//!
//! like `docker exec -it` but for the local stack: parent env merged with
//! service env + the service's .env files, cwd set, PS1 prefixed so you know
//! which one you're in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::{Config, ServiceSpec};

/// everything we need to actually exec into the shell. returned separately so
/// tests can verify the resolved env without forking.
#[derive(Debug)]
pub struct ShellPlan {
    pub shell: PathBuf,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    /// for bash/sh we inject PS1 via the plan.
    pub ps1: String,
    /// path to a temp zshrc if we wrote one (zsh won't honor PS1 by default).
    pub zshrc: Option<PathBuf>,
}

/// resolve plan (but don't exec). returns an error if the service doesn't
/// exist in config.
pub fn plan(
    cfg: &Config,
    service: &str,
    parent_env: &HashMap<String, String>,
) -> Result<ShellPlan, String> {
    let spec = cfg
        .services
        .iter()
        .find(|s| s.name == service)
        .ok_or_else(|| format!("no service named `{service}` in config"))?;
    let shell = parent_env
        .get("SHELL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    let cwd = spec
        .cwd
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let env = resolve_env(spec, &cwd, parent_env);
    let ps1 = format!("[pulse:{}] \\$ ", service);
    Ok(ShellPlan {
        shell,
        cwd,
        env,
        ps1,
        zshrc: None,
    })
}

/// merge parent env, service env, and .env file contents. later sources win.
pub fn resolve_env(
    spec: &ServiceSpec,
    cwd: &Path,
    parent: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut out = parent.clone();
    // .env files first (so config env can still override)
    for f in crate::envwatch::present_env_files(cwd) {
        if let Ok(raw) = std::fs::read_to_string(&f) {
            for (k, v) in parse_dotenv(&raw) {
                out.insert(k, v);
            }
        }
    }
    for (k, v) in &spec.env {
        out.insert(k.clone(), v.clone());
    }
    out
}

/// tiny .env parser: `KEY=VALUE` per line, `#` starts a comment, blank lines
/// ignored, quotes stripped. not a full spec — good enough for devs.
pub fn parse_dotenv(raw: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            let k = k.trim().to_string();
            let mut v = v.trim().to_string();
            if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
                || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
            {
                v = v[1..v.len() - 1].to_string();
            }
            out.push((k, v));
        }
    }
    out
}

/// for zsh specifically: write a temp zshrc that sets PS1, then point ZDOTDIR
/// at that tempdir.
pub fn prepare_zshrc(ps1: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("pulse-zsh-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let rc = dir.join(".zshrc");
    // source user's actual zshrc first if present, then override PS1
    let home = std::env::var("HOME").unwrap_or_default();
    let body = format!("[ -f {home}/.zshrc ] && source {home}/.zshrc\nPS1='{ps1}'\n");
    std::fs::write(&rc, body)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceSpec;

    fn mk_spec(name: &str, cwd: Option<PathBuf>, env: &[(&str, &str)]) -> ServiceSpec {
        let mut m = HashMap::new();
        for (k, v) in env {
            m.insert((*k).into(), (*v).into());
        }
        ServiceSpec {
            name: name.into(),
            cmd: "x".into(),
            cwd,
            env: m,
            color: None,
            probe: None,
            port: None,
            agent: None,
            depends_on: Vec::new(),
            tap: None,
            auto_restart: None,
            watch_env: None,
        }
    }

    #[test]
    fn config_env_overrides_parent() {
        let dir = tempfile::tempdir().unwrap();
        let spec = mk_spec(
            "api",
            Some(dir.path().to_path_buf()),
            &[("FOO", "fromspec")],
        );
        let mut parent = HashMap::new();
        parent.insert("FOO".into(), "fromshell".into());
        let merged = resolve_env(&spec, dir.path(), &parent);
        assert_eq!(merged.get("FOO").unwrap(), "fromspec");
    }

    #[test]
    fn dotenv_overrides_parent_spec_overrides_dotenv() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "DB=sqlite\nAPI_KEY=abc").unwrap();
        let spec = mk_spec("api", Some(dir.path().to_path_buf()), &[("DB", "postgres")]);
        let parent = HashMap::new();
        let merged = resolve_env(&spec, dir.path(), &parent);
        assert_eq!(merged.get("DB").unwrap(), "postgres");
        assert_eq!(merged.get("API_KEY").unwrap(), "abc");
    }

    #[test]
    fn parse_dotenv_handles_quotes_and_comments() {
        let raw = "# leading comment\nA=1\nB=\"quoted value\"\nC='single'\n\n";
        let kv = parse_dotenv(raw);
        assert_eq!(kv.len(), 3);
        assert_eq!(kv[0], ("A".into(), "1".into()));
        assert_eq!(kv[1], ("B".into(), "quoted value".into()));
        assert_eq!(kv[2], ("C".into(), "single".into()));
    }

    #[test]
    fn plan_errors_on_unknown_service() {
        let cfg = crate::config::parse(
            r#"
            [[service]]
            name = "api"
            cmd = "x"
            "#,
        )
        .unwrap();
        let parent = HashMap::new();
        assert!(plan(&cfg, "nope", &parent).is_err());
    }

    #[test]
    fn plan_sets_ps1_with_service_name() {
        let cfg = crate::config::parse(
            r#"
            [[service]]
            name = "web"
            cmd = "x"
            "#,
        )
        .unwrap();
        let parent = HashMap::new();
        let p = plan(&cfg, "web", &parent).unwrap();
        assert!(p.ps1.contains("pulse:web"));
    }
}
