use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(rename = "service", default)]
    pub services: Vec<ServiceSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceSpec {
    pub name: String,
    pub cmd: String,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub color: Option<String>,
}

impl ServiceSpec {
    pub fn parse_cmd(&self) -> Result<Vec<String>, ConfigError> {
        let parts =
            shlex::split(&self.cmd).ok_or_else(|| ConfigError::BadCmd(self.name.clone()))?;
        if parts.is_empty() {
            return Err(ConfigError::EmptyCmd(self.name.clone()));
        }
        Ok(parts)
    }
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
    for s in &cfg.services {
        // validate cmd up front
        let _ = s.parse_cmd()?;
        if !seen.insert(s.name.clone()) {
            return Err(ConfigError::Duplicate(s.name.clone()));
        }
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
}
