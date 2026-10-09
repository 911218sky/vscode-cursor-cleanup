//! Ink + Amber Signal theme (BossTerm-inspired, scarce accent).

use crate::paths::Risk;
use ratatui::style::{Color, Modifier, Style};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const DOTS: [&str; 4] = ["   ", ".  ", ".. ", "..."];

/// Primary signal — amber, scarce use only.
pub const ACCENT: Color = Color::Rgb(242, 169, 59);
const BORDER: Color = Color::Rgb(42, 55, 68);
/// Soft wash — keep selection light so the UI doesn't feel blocked-in.
const SELECT_BG: Color = Color::Rgb(32, 28, 24);
const DATA: Color = Color::Rgb(86, 199, 224);
const OK: Color = Color::Rgb(111, 208, 140);
const WARN: Color = Color::Rgb(240, 180, 41);
const ALERT: Color = Color::Rgb(242, 104, 95);

pub fn spinner(frame: u64) -> &'static str {
    SPINNER[(frame as usize / 2) % SPINNER.len()]
}

pub fn dots(frame: u64) -> &'static str {
    DOTS[(frame as usize / 3) % DOTS.len()]
}

pub fn accent(_frame: u64, _offset: usize) -> Color {
    ACCENT
}

pub fn accent_style(_frame: u64, _offset: usize) -> Style {
    Style::default()
        .fg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn border_style(_frame: u64) -> Style {
    Style::default().fg(BORDER)
}

pub fn focus_border_style() -> Style {
    Style::default().fg(ACCENT)
}

pub fn select_style() -> Style {
    Style::default()
        .bg(SELECT_BG)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

pub fn data_style() -> Style {
    Style::default().fg(DATA)
}

pub fn risk_color(risk: Risk) -> Color {
    match risk {
        Risk::Safe => OK,
        Risk::Medium => WARN,
        Risk::High => ALERT,
    }
}

pub fn ok_color() -> Color {
    OK
}

pub fn warn_color() -> Color {
    WARN
}

pub fn progress_bar(ratio: f64, width: usize, _frame: u64) -> String {
    let width = width.max(8);
    let filled = ((ratio.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    let mut out = String::with_capacity(width);
    for i in 0..width {
        out.push(if i < filled { '━' } else { '─' });
    }
    out
}

pub fn pct_label(ratio: f64) -> String {
    format!("{:>3.0}%", ratio * 100.0)
}
