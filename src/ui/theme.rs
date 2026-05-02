use ratatui::style::Color;
use ratatui::widgets::BorderType;

/// tokyonight-ish defaults. not pixel-perfect, just dark and readable.
#[derive(Debug, Clone)]
pub struct Palette {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub dim: Color,
    pub ok: Color,
    pub warn: Color,
    pub error: Color,
    pub sidebar_sel_bg: Color,
    pub border_color: Color,
    pub border_type: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            bg: Color::Rgb(15, 17, 22),
            fg: Color::Rgb(192, 202, 245),
            accent: Color::Rgb(94, 234, 212),
            dim: Color::Rgb(130, 139, 172),
            ok: Color::Rgb(158, 206, 106),
            warn: Color::Rgb(224, 175, 104),
            error: Color::Rgb(247, 118, 142),
            sidebar_sel_bg: Color::Rgb(40, 44, 60),
            border_color: Color::Rgb(86, 95, 137),
            border_type: "rounded".into(),
        }
    }
}

impl Palette {
    pub fn border(&self) -> BorderType {
        match self.border_type.as_str() {
            "double" => BorderType::Double,
            "plain" => BorderType::Plain,
            "thick" => BorderType::Thick,
            _ => BorderType::Rounded,
        }
    }
}

// legacy module-level constants kept for existing call sites.
pub const BG_SEL: Color = Color::Rgb(40, 44, 60);
pub const BORDER: Color = Color::Rgb(86, 95, 137);
pub const DIM: Color = Color::Rgb(130, 139, 172);
pub const FG: Color = Color::Rgb(192, 202, 245);

pub const RUNNING: Color = Color::Rgb(158, 206, 106);
pub const STARTING: Color = Color::Rgb(224, 175, 104);
pub const CRASHED: Color = Color::Rgb(247, 118, 142);
pub const STOPPED: Color = Color::Rgb(86, 95, 137);

pub fn from_name(name: Option<&str>) -> Color {
    match name.map(|s| s.to_lowercase()) {
        Some(s) if s == "cyan" => Color::Cyan,
        Some(s) if s == "green" => Color::Green,
        Some(s) if s == "yellow" => Color::Yellow,
        Some(s) if s == "magenta" => Color::Magenta,
        Some(s) if s == "blue" => Color::Blue,
        Some(s) if s == "red" => Color::Red,
        Some(s) if s == "white" => Color::White,
        _ => Color::Cyan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_palette_is_dark() {
        let p = Palette::default();
        if let Color::Rgb(r, g, b) = p.bg {
            let sum: u32 = r as u32 + g as u32 + b as u32;
            assert!(sum < 200);
        }
    }

    #[test]
    fn border_type_maps() {
        let p = Palette {
            border_type: "double".into(),
            ..Palette::default()
        };
        assert!(matches!(p.border(), BorderType::Double));
        let p = Palette {
            border_type: "thick".into(),
            ..Palette::default()
        };
        assert!(matches!(p.border(), BorderType::Thick));
        let p = Palette {
            border_type: "plain".into(),
            ..Palette::default()
        };
        assert!(matches!(p.border(), BorderType::Plain));
        let p = Palette {
            border_type: "zigzag".into(),
            ..Palette::default()
        };
        assert!(matches!(p.border(), BorderType::Rounded));
    }

    #[test]
    fn from_name_known() {
        assert_eq!(from_name(Some("cyan")), Color::Cyan);
        assert_eq!(from_name(Some("RED")), Color::Red);
    }

    #[test]
    fn from_name_unknown_default_cyan() {
        assert_eq!(from_name(Some("chartreuse")), Color::Cyan);
        assert_eq!(from_name(None), Color::Cyan);
    }
}
