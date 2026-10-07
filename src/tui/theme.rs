//! Visual theme + frame-based animation helpers (restrained palette).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const DOTS: [&str; 4] = ["   ", ".  ", ".. ", "..."];

/// Primary accent — fixed cyan, no rainbow cycling.
pub const ACCENT: Color = Color::Cyan;
const BORDER: Color = Color::Rgb(70, 90, 110);
const SELECT_BG: Color = Color::Rgb(35, 42, 52);

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

pub fn title_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
}

pub fn select_bg() -> Color {
    SELECT_BG
}

pub fn progress_bar(ratio: f64, width: usize, _frame: u64) -> String {
    let width = width.max(8);
    let filled = ((ratio.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    let mut out = String::with_capacity(width + 2);
    out.push('[');
    for i in 0..width {
        out.push(if i < filled { '█' } else { '░' });
    }
    out.push(']');
    out
}

pub fn pct_label(ratio: f64) -> String {
    format!("{:>3.0}%", ratio * 100.0)
}
