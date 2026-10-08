//! Plain stdout output for `--scan` / `--yes` CLI mode (non-TUI).

use crate::cleanup::{collect_stats, is_editor_running, run_target, TargetStat};
use crate::fsutil::{fmt_size, path_size};
use crate::i18n::{self, t, Msg};
use crate::paths::{Editor, Risk};
use std::io::{self, Write};
use unicode_width::UnicodeWidthStr;

const VERSION: &str = env!("CARGO_PKG_VERSION");

mod ansi {
    pub fn wrap(s: &str, code: &str) -> String {
        if std::env::var("NO_COLOR").is_ok() {
            return s.to_string();
        }
        format!("\x1b[{code}m{s}\x1b[0m")
    }
    pub fn green(s: &str) -> String {
        wrap(s, "32")
    }
    pub fn green_bold(s: &str) -> String {
        wrap(s, "1;32")
    }
    pub fn yellow(s: &str) -> String {
        wrap(s, "33")
    }
    pub fn yellow_bold(s: &str) -> String {
        wrap(s, "1;33")
    }
    pub fn red_bold(s: &str) -> String {
        wrap(s, "1;31")
    }
    pub fn cyan(s: &str) -> String {
        wrap(s, "36")
    }
    pub fn bright_cyan(s: &str) -> String {
        wrap(s, "96")
    }
    pub fn bright_cyan_bold(s: &str) -> String {
        wrap(s, "1;96")
    }
    pub fn bright_magenta_bold(s: &str) -> String {
        wrap(s, "1;95")
    }
    pub fn bright_yellow_bold(s: &str) -> String {
        wrap(s, "1;93")
    }
    pub fn bright_green(s: &str) -> String {
        wrap(s, "92")
    }
    pub fn dim(s: &str) -> String {
        wrap(s, "2")
    }
    pub fn white_bold(s: &str) -> String {
        wrap(s, "1;37")
    }
    pub fn bright_black(s: &str) -> String {
        wrap(s, "90")
    }
}

fn pad_center(s: &str, width: usize) -> String {
    let sw = s.width();
    if sw >= width {
        return s.to_string();
    }
    let pad = width - sw;
    let left = pad / 2;
    let right = pad - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

pub fn print_banner() {
    let w = 48;
    let line = "─".repeat(w);
    println!();
    println!("{}", ansi::bright_cyan(&format!("┌{line}┐")));
    println!(
        "{}",
        ansi::bright_cyan_bold(&format!("│{}│", pad_center("cursor-cleanup", w)))
    );
    println!(
        "{}",
        ansi::cyan(&format!(
            "│{}│",
            pad_center(&format!("Cursor  ·  v{VERSION}"), w)
        ))
    );
    println!(
        "{}",
        ansi::dim(&format!("│{}│", pad_center(t(Msg::BannerHint), w)))
    );
    println!("{}", ansi::bright_cyan(&format!("└{line}┘")));
    println!();
}

pub fn ok(msg: &str) {
    println!(
        "  {} {}",
        ansi::green_bold("✔"),
        ansi::green_bold(msg)
    );
}

pub fn warn(msg: &str) {
    println!("  {} {}", ansi::yellow_bold("⚠"), ansi::yellow(msg));
}

pub fn pause_exit() {
    println!();
    print!("  {} ", ansi::bright_black(t(Msg::PauseExit)));
    let _ = io::stdout().flush();
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s);
}

pub fn pause_menu() {
    println!();
    print!("  {} ", ansi::bright_black(t(Msg::PressEnterMenu)));
    let _ = io::stdout().flush();
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s);
}

pub fn print_scan(editor: Editor, root: &std::path::Path, total: u64, stats: &[TargetStat]) {
    println!();
    let title = format!("── {} ", editor.display_name());
    let fill = 40usize.saturating_sub(title.width());
    println!(
        "  {}",
        ansi::bright_magenta_bold(&format!("{title}{}", "─".repeat(fill)))
    );
    println!("  {}", ansi::dim(&root.display().to_string()));
    println!(
        "  {}  {}",
        ansi::dim(t(Msg::TotalSize)),
        ansi::white_bold(&fmt_size(total))
    );
    println!();
    for (i, s) in stats.iter().enumerate() {
        let tag = match s.target.risk {
            Risk::Safe => ansi::green(i18n::risk_tag(Risk::Safe)),
            Risk::Medium => ansi::yellow(i18n::risk_tag(Risk::Medium)),
            Risk::High => ansi::red_bold(i18n::risk_tag(Risk::High)),
        };
        let title = i18n::target_title(s.target.kind);
        println!(
            "  {:>2}. {:<24} {:>10}  {}",
            i + 1,
            title,
            fmt_size(s.bytes),
            tag
        );
    }
    let cleanable: u64 = stats.iter().map(|s| s.bytes).sum();
    println!();
    println!(
        "  {}  {}",
        ansi::dim(t(Msg::Cleanable)),
        ansi::bright_yellow_bold(&fmt_size(cleanable))
    );
    println!();
}

fn idx_risk(stats: &[TargetStat], pred: impl Fn(Risk) -> bool) -> Vec<usize> {
    stats
        .iter()
        .enumerate()
        .filter(|(_, s)| pred(s.target.risk))
        .map(|(i, _)| i)
        .collect()
}

/// Non-interactive clean (`--yes`): safe items only, skips if editor is running.
/// Returns `true` only when clean ran without per-target errors.
pub fn run_yes_clean(editor: Editor) -> bool {
    let Some((root, total, stats)) = collect_stats(editor) else {
        warn(&i18n::fmt_no_resolve(editor.display_name()));
        return false;
    };
    if !root.exists() {
        warn(&i18n::fmt_no_dir(
            editor.display_name(),
            &root.display().to_string(),
        ));
        return false;
    }
    print_scan(editor, &root, total, &stats);

    if is_editor_running(editor) {
        warn(t(Msg::EditorRunningAbort));
        return false;
    }

    let selected = idx_risk(&stats, |r| matches!(r, Risk::Safe));
    if selected.is_empty() {
        ok(t(Msg::AllDone));
        return true;
    }

    let mut freed_total = 0u64;
    let mut any_errors = false;
    for &idx in &selected {
        let target = &stats[idx].target;
        let title = i18n::target_title(target.kind);
        print!("  {} {} ... ", ansi::cyan("→"), title);
        let _ = io::stdout().flush();
        let result = run_target(&root, target);
        freed_total += result.freed;
        if result.errors == 0 {
            println!(
                "{} {}",
                ansi::green_bold(t(Msg::Done)),
                fmt_size(result.freed)
            );
        } else {
            any_errors = true;
            if result.remaining > 0 {
                println!(
                    "{}",
                    ansi::yellow_bold(&i18n::fmt_clean_residual(
                        &fmt_size(result.freed),
                        &fmt_size(result.remaining)
                    ))
                );
            } else {
                println!(
                    "{}",
                    ansi::yellow_bold(&i18n::fmt_partial(
                        &fmt_size(result.freed),
                        result.errors
                    ))
                );
            }
        }
    }

    let after = path_size(&root);
    println!();
    println!(
        "  {}",
        ansi::bright_green("────────────────────────────────")
    );
    ok(&i18n::fmt_summary(
        editor.display_name(),
        &fmt_size(total),
        &fmt_size(after),
        &fmt_size(freed_total),
    ));
    println!(
        "  {}",
        ansi::bright_green("────────────────────────────────")
    );
    !any_errors
}
