use pulse::theme_file::{dump_default, ThemeFile};

#[test]
fn dump_is_valid_toml() {
    let s = dump_default();
    toml::from_str::<ThemeFile>(&s).unwrap();
}

#[test]
fn dump_mentions_all_colors() {
    let s = dump_default();
    for field in ["bg", "fg", "accent", "dim", "ok", "warn", "error"] {
        assert!(s.contains(field), "missing {field}");
    }
    assert!(s.contains("sidebar_selected_bg"));
    assert!(s.contains("border_type"));
}

#[test]
fn dump_contains_tokyonight_bg_hex() {
    let s = dump_default();
    // default bg from Palette::default
    assert!(s.contains("0f1116"));
}

#[test]
fn rejects_extra_keys() {
    let raw = "[colors]\nflamingo = \"#ff00ff\"\n";
    assert!(toml::from_str::<ThemeFile>(raw).is_err());
}
