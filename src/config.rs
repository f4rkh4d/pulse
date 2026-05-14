use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("no [[service]] entries in config")]
    Empty,
    #[error("service `{0}`: empty cmd")]
    EmptyCmd(String),
    #[error("service `{0}`: cmd couldn't be split (unterminated quote?)")]
    BadCmd(String),
    #[error("duplicate service name `{0}`")]
    Duplicate(String),
    #[error("service `{svc}`: bad duration `{val}` (try `500ms`, `2s`, `1m`)")]
    BadDuration { svc: String, val: String },
    #[error("service `{svc}`: depends on unknown service `{dep}`")]
    UnknownDep { svc: String, dep: String },
    #[error("service `{svc}`: depends on itself")]
    SelfDep { svc: String },
    #[error("circular dependency involving `{0}`")]
    Cycle(String),
    #[error("service `{svc}`: unknown agent kind `{kind}` (goblin, cat, ghost, robot, blob)")]
    BadAgent { svc: String, kind: String },
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(rename = "service", default)]
    pub services: Vec<ServiceSpec>,
    #[serde(default)]
    pub global: Option<GlobalSpec>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GlobalSpec {
    #[serde(default)]
    pub stop_timeout: Option<String>,
    #[serde(default)]
    pub log_buffer: Option<usize>,
}

impl GlobalSpec {
    pub fn stop_timeout_dur(&self) -> Duration {
        self.stop_timeout
            .as_deref()
            .and_then(parse_duration)
            .unwrap_or_else(|| Duration::from_millis(1500))
    }
}

impl Config {
    /// default to 2000 when unspecified. used to size the per-service ring.
    pub fn log_buffer_size(&self) -> usize {
        self.global
            .as_ref()
            .and_then(|g| g.log_buffer)
            .unwrap_or(2000)
            .max(1)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceSpec {
    pub name: String,
    pub cmd: String,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub probe: Option<ProbeSpec>,
    #[serde(default)]
    pub port: Option<PortSpec>,
    #[serde(default)]
    pub agent: Option<AgentSpec>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub tap: Option<TapSpec>,
    /// restart the service on unexpected exit with exponential backoff.
    /// default true. set `auto_restart = false` to opt out.
    #[serde(default)]
    pub auto_restart: Option<bool>,
    /// watch `.env`, `.env.local`, `.env.development` in cwd and restart on
    /// change. default true when any of those files exist at startup.
    #[serde(default)]
    pub watch_env: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TapSpec {
    #[serde(default = "default_tap_mode")]
    pub mode: String,
    /// port pulse listens on when mode = proxy. forwards to the probe target.
    #[serde(default)]
    pub listen: Option<u16>,
    /// target port on localhost; defaults to port.expect or parsed from probe url.
    #[serde(default)]
    pub target: Option<u16>,
}

fn default_tap_mode() -> String {
    "proxy".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeSpec {
    pub url: String,
    #[serde(default = "default_interval")]
    pub interval: String,
    #[serde(default = "default_timeout")]
    pub timeout: String,
    #[serde(default)]
    pub expect_status: Option<u16>,
}

fn default_interval() -> String {
    "5s".into()
}
fn default_timeout() -> String {
    "2s".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortSpec {
    pub expect: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSpec {
    #[serde(default = "default_agent_kind")]
    pub kind: String,
}

fn default_agent_kind() -> String {
    "goblin".into()
}

impl ServiceSpec {
    pub fn auto_restart_enabled(&self) -> bool {
        self.auto_restart.unwrap_or(true)
    }
    pub fn watch_env_enabled(&self) -> bool {
        self.watch_env.unwrap_or(true)
    }

    pub fn parse_cmd(&self) -> Result<Vec<String>, ConfigError> {
        let parts =
            shlex::split(&self.cmd).ok_or_else(|| ConfigError::BadCmd(self.name.clone()))?;
        if parts.is_empty() {
            return Err(ConfigError::EmptyCmd(self.name.clone()));
        }
        Ok(parts)
    }

    pub fn probe_interval(&self) -> Result<Duration, ConfigError> {
        match &self.probe {
            Some(p) => parse_duration(&p.interval).ok_or(ConfigError::BadDuration {
                svc: self.name.clone(),
                val: p.interval.clone(),
            }),
            None => Ok(Duration::from_secs(5)),
        }
    }

    pub fn probe_timeout(&self) -> Result<Duration, ConfigError> {
        match &self.probe {
            Some(p) => parse_duration(&p.timeout).ok_or(ConfigError::BadDuration {
                svc: self.name.clone(),
                val: p.timeout.clone(),
            }),
            None => Ok(Duration::from_secs(2)),
        }
    }
}

/// parse `500ms`, `2s`, `1m`, `1h`. returns None on garbage.
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num, suffix) = split_suffix(s);
    let n: u64 = num.parse().ok()?;
    match suffix {
        "ms" => Some(Duration::from_millis(n)),
        "s" | "" => Some(Duration::from_secs(n)),
        "m" => Some(Duration::from_secs(n * 60)),
        "h" => Some(Duration::from_secs(n * 3600)),
        _ => None,
    }
}

fn split_suffix(s: &str) -> (&str, &str) {
    let idx = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    (&s[..idx], &s[idx..])
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let raw = std::fs::read_to_string(path)?;
    parse(&raw)
}

pub fn parse(raw: &str) -> Result<Config, ConfigError> {
    let cfg: Config = toml::from_str(raw)?;
    if cfg.services.is_empty() {
        return Err(ConfigError::Empty);
    }
    let mut seen = std::collections::HashSet::new();
    let valid_kinds = ["goblin", "cat", "ghost", "robot", "blob"];
    for s in &cfg.services {
        let _ = s.parse_cmd()?;
        let _ = s.probe_interval()?;
        let _ = s.probe_timeout()?;
        if let Some(a) = &s.agent {
            if !valid_kinds.contains(&a.kind.as_str()) {
                return Err(ConfigError::BadAgent {
                    svc: s.name.clone(),
                    kind: a.kind.clone(),
                });
            }
        }
        if !seen.insert(s.name.clone()) {
            return Err(ConfigError::Duplicate(s.name.clone()));
        }
    }
    // validate depends_on references exist and no self-loops
    let names: std::collections::HashSet<&str> =
        cfg.services.iter().map(|s| s.name.as_str()).collect();
    for s in &cfg.services {
        for dep in &s.depends_on {
            if dep == &s.name {
                return Err(ConfigError::SelfDep {
                    svc: s.name.clone(),
                });
            }
            if !names.contains(dep.as_str()) {
                return Err(ConfigError::UnknownDep {
                    svc: s.name.clone(),
                    dep: dep.clone(),
                });
            }
        }
    }
    // cycle detection
    if let Some(bad) = crate::deps::find_cycle(&cfg.services) {
        return Err(ConfigError::Cycle(bad));
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic() {
        let raw = r#"
            [[service]]
            name = "api"
            cmd = "cargo run"
            cwd = "./backend"
            color = "cyan"

            [[service]]
            name = "web"
            cmd = "npm run dev"
        "#;
        let cfg = parse(raw).unwrap();
        assert_eq!(cfg.services.len(), 2);
        assert_eq!(cfg.services[0].name, "api");
        assert_eq!(cfg.services[0].parse_cmd().unwrap(), vec!["cargo", "run"]);
    }

    #[test]
    fn rejects_empty() {
        assert!(parse("").is_err());
    }

    #[test]
    fn rejects_dup() {
        let raw = r#"
            [[service]]
            name = "x"
            cmd = "a"
            [[service]]
            name = "x"
            cmd = "b"
        "#;
        assert!(parse(raw).is_err());
    }

    #[test]
    fn parses_env() {
        let raw = r#"
            [[service]]
            name = "api"
            cmd = "./run"
            env = { PORT = "3000", HOST = "0.0.0.0" }
        "#;
        let cfg = parse(raw).unwrap();
        assert_eq!(cfg.services[0].env.get("PORT").unwrap(), "3000");
    }

    #[test]
    fn shlex_quoted() {
        let raw = r#"
            [[service]]
            name = "x"
            cmd = "sh -c 'echo hi there'"
        "#;
        let cfg = parse(raw).unwrap();
        let parts = cfg.services[0].parse_cmd().unwrap();
        assert_eq!(parts, vec!["sh", "-c", "echo hi there"]);
    }

    #[test]
    fn parses_probe_and_port() {
        let raw = r#"
            [[service]]
            name = "api"
            cmd = "./run"

            [service.probe]
            url = "http://localhost:3000/health"
            interval = "5s"
            timeout = "2s"

            [service.port]
            expect = 3000
        "#;
        let cfg = parse(raw).unwrap();
        assert_eq!(cfg.services[0].port.as_ref().unwrap().expect, 3000);
        assert_eq!(
            cfg.services[0].probe.as_ref().unwrap().url,
            "http://localhost:3000/health"
        );
    }

    #[test]
    fn rejects_bad_duration() {
        let raw = r#"
            [[service]]
            name = "x"
            cmd = "./run"

            [service.probe]
            url = "http://x"
            interval = "soon"
        "#;
        assert!(parse(raw).is_err());
    }

    #[test]
    fn duration_parser_covers_units() {
        assert_eq!(parse_duration("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_duration("2s"), Some(Duration::from_secs(2)));
        assert_eq!(parse_duration("1m"), Some(Duration::from_secs(60)));
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn rejects_bad_agent_kind() {
        let raw = r#"
            [[service]]
            name = "x"
            cmd = "./run"
            [service.agent]
            kind = "dragon"
        "#;
        assert!(parse(raw).is_err());
    }

    #[test]
    fn rejects_self_dep() {
        let raw = r#"
            [[service]]
            name = "x"
            cmd = "./run"
            depends_on = ["x"]
        "#;
        assert!(parse(raw).is_err());
    }

    #[test]
    fn rejects_unknown_dep() {
        let raw = r#"
            [[service]]
            name = "x"
            cmd = "./run"
            depends_on = ["nope"]
        "#;
        assert!(parse(raw).is_err());
    }

    #[test]
    fn rejects_misspelled_keys() {
        let raw = r#"
            [[service]]
            name = "x"
            cmd = "./run"
            colour = "cyan"
        "#;
        assert!(parse(raw).is_err());
    }
}
