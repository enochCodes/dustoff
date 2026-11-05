use ratatui::style::Color;

use crate::rules::{Cat, Risk};

pub const BG: Color = Color::Rgb(24, 26, 31);
pub const PANEL: Color = Color::Rgb(32, 35, 42);
pub const SEL: Color = Color::Rgb(44, 49, 60);
pub const FG: Color = Color::Rgb(219, 223, 230);
pub const DIM: Color = Color::Rgb(104, 112, 128);
pub const FAINT: Color = Color::Rgb(66, 72, 84);
pub const ACCENT: Color = Color::Rgb(94, 196, 209);
pub const ACCENT_DK: Color = Color::Rgb(48, 118, 128);

pub const SAFE: Color = Color::Rgb(132, 198, 130);
pub const REBUILD: Color = Color::Rgb(226, 182, 106);
pub const CAREFUL: Color = Color::Rgb(228, 118, 122);
pub const BLOCKED: Color = Color::Rgb(128, 96, 160);

pub fn risk_color(r: Risk) -> Color {
    match r {
        Risk::Safe => SAFE,
        Risk::Rebuildable => REBUILD,
        Risk::Careful => CAREFUL,
    }
}

pub fn cat_color(c: Cat) -> Color {
    match c {
        Cat::Agents => Color::Rgb(168, 158, 232),
        Cat::Dev => ACCENT,
        Cat::Mobile => Color::Rgb(226, 182, 106),
        Cat::Browsers => Color::Rgb(134, 190, 232),
        Cat::Apps => Color::Rgb(146, 196, 160),
        Cat::Containers => Color::Rgb(116, 174, 226),
        Cat::System => Color::Rgb(186, 186, 196),
        Cat::Downloads => Color::Rgb(226, 150, 120),
        Cat::Projects => Color::Rgb(202, 166, 136),
        Cat::Manual => DIM,
    }
}

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Eighth-blocks, for sub-character bar resolution.
pub const EIGHTHS: [&str; 9] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

/// Draw a proportional bar `width` cells wide using partial blocks.
pub fn bar(frac: f64, width: usize) -> String {
    let frac = frac.clamp(0.0, 1.0);
    let total = (frac * width as f64 * 8.0).round() as usize;
    let full = total / 8;
    let rem = total % 8;
    let mut s = "█".repeat(full.min(width));
    if full < width && rem > 0 {
        s.push_str(EIGHTHS[rem]);
    }
    let drawn = full.min(width) + usize::from(full < width && rem > 0);
    s.push_str(&"·".repeat(width.saturating_sub(drawn)));
    s
}
