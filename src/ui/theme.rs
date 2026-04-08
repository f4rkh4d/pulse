use ratatui::style::Color;

/// tokyonight-ish defaults. not pixel-perfect, just dark and readable.
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
