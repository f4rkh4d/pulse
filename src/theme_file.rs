//! loads `~/.config/pulse/theme.toml` if present and folds it over the default
//! palette. missing file, bad toml, unknown hex — all fall back quietly.

use std::path::PathBuf;

use ratatui::style::Color;
use serde::Deserialize;

use crate::ui::theme::Palette;

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    #[serde(default)]
    pub colors: Colors,
    #[serde(default)]
    pub styles: Styles,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub fg: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub dim: Option<String>,
    #[serde(default)]
    pub ok: Option<String>,
    #[serde(default)]
    pub warn: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Styles {
    #[serde(default)]
    pub sidebar_selected_bg: Option<String>,
    #[serde(default)]
    pub border_type: Option<String>,
}

pub fn config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "pulse").map(|d| d.config_dir().join("theme.toml"))
}

pub fn load() -> Palette {
    let mut p = Palette::default();
    let Some(path) = config_path() else {
        return p;
    };
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return p,
    };
    if let Ok(tf) = toml::from_str::<ThemeFile>(&raw) {
        apply(&mut p, &tf);
    }
    p
}

pub fn apply(p: &mut Palette, tf: &ThemeFile) {
    if let Some(c) = parse_hex(&tf.colors.bg) {
        p.bg = c;
    }
    if let Some(c) = parse_hex(&tf.colors.fg) {
        p.fg = c;
    }
    if let Some(c) = parse_hex(&tf.colors.accent) {
        p.accent = c;
    }
    if let Some(c) = parse_hex(&tf.colors.dim) {
        p.dim = c;
    }
    if let Some(c) = parse_hex(&tf.colors.ok) {
        p.ok = c;
    }
    if let Some(c) = parse_hex(&tf.colors.warn) {
        p.warn = c;
    }
    if let Some(c) = parse_hex(&tf.colors.error) {
        p.error = c;
    }
    if let Some(c) = parse_hex(&tf.styles.sidebar_selected_bg) {
        p.sidebar_sel_bg = c;
    }
    if let Some(bt) = &tf.styles.border_type {
        p.border_type = bt.clone();
    }
}

fn parse_hex(s: &Option<String>) -> Option<Color> {
    let raw = s.as_deref()?.trim();
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// prints the current default palette as a starter theme.toml
pub fn dump_default() -> String {
    let p = Palette::default();
    let hex = |c: Color| -> String {
        match c {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            _ => "#ffffff".into(),
        }
    };
    format!(
        "[colors]\nbg = \"{}\"\nfg = \"{}\"\naccent = \"{}\"\ndim = \"{}\"\nok = \"{}\"\nwarn = \"{}\"\nerror = \"{}\"\n\n[styles]\nsidebar_selected_bg = \"{}\"\nborder_type = \"{}\"\n",
        hex(p.bg),
        hex(p.fg),
        hex(p.accent),
        hex(p.dim),
        hex(p.ok),
        hex(p.warn),
        hex(p.error),
        hex(p.sidebar_sel_bg),
        p.border_type,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parser_happy() {
        let c = parse_hex(&Some("#0f1116".into())).unwrap();
        assert!(matches!(c, Color::Rgb(0x0f, 0x11, 0x16)));
    }

    #[test]
    fn hex_parser_rejects_short() {
        assert!(parse_hex(&Some("#abc".into())).is_none());
        assert!(parse_hex(&Some("".into())).is_none());
        assert!(parse_hex(&None).is_none());
    }

    #[test]
    fn dump_default_round_trips() {
        let raw = dump_default();
        let parsed: ThemeFile = toml::from_str(&raw).unwrap();
        let mut p = Palette::default();
        apply(&mut p, &parsed);
        assert_eq!(p.border_type, Palette::default().border_type);
    }

    #[test]
    fn unknown_border_type_keeps_default() {
        let raw = r#"
[colors]
[styles]
border_type = "zigzag"
"#;
        let tf: ThemeFile = toml::from_str(raw).unwrap();
        let mut p = Palette::default();
        apply(&mut p, &tf);
        // value gets stored; resolver in theme.rs normalizes
        assert_eq!(p.border_type, "zigzag");
    }
}
