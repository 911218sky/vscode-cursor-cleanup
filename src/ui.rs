use crate::backup::{self, BackupInfo};
use crate::cleanup::{collect_stats, force_quit_editor, is_editor_running, run_target, TargetStat};
use crate::fsutil::{fmt_size, path_size};
use crate::i18n::{self, t, Lang, Msg};
use crate::paths::{Editor, Risk};
use colored::Colorize;
use console::Term;
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect, Select};
use std::io::{self, Write};
use unicode_width::UnicodeWidthStr;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

/// Pad / center by terminal display columns (CJK = 2), not Rust char count.
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

/// Prompt for a selection. Returns `None` on cancel / I/O error (never treats
/// failure as confirming a destructive default).
fn ask(prompt: &str, items: &[&str], default: usize) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let default = default.min(items.len() - 1);
    Select::with_theme(&theme())
        .with_prompt(prompt)
        .items(items)
        .default(default)
        .interact()
        .ok()
}

/// Ensure the editor is not holding locks. Auto force-quit — no confirm spam.
/// Returns `false` when quit failed and the user aborts.
fn ensure_editor_ready(editor: Editor) -> bool {
    if !is_editor_running(editor) {
        return true;
    }

    warn(&i18n::fmt_running(editor.display_name()));
    print!("  {} {} ", "…".cyan(), t(Msg::ForceQuitWait).dimmed());
    let _ = io::stdout().flush();
    let ok_quit = force_quit_editor(editor);
    println!();
    if ok_quit {
        ok(&format!("{} {}", t(Msg::ForceQuitOk), editor.display_name()));
        return true;
    }
    warn(t(Msg::ForceQuitFail));
    match ask(
        t(Msg::StillContinue),
        &[t(Msg::Continue), t(Msg::Back)],
        1,
    ) {
        Some(0) => true,
        _ => false,
    }
}

/// Clear terminal and reprint banner (fresh screen for next step).
pub fn clear_screen() {
    let term = Term::stdout();
    let _ = term.clear_screen();
    let _ = term.move_cursor_to(0, 0);
    print_banner();
}

pub fn pick_language_interactive() {
    let idx = ask(
        t(Msg::PickLang),
        &[t(Msg::LangTw), t(Msg::LangCn), t(Msg::LangEn)],
        0,
    )
    .unwrap_or(0);
    let lang = match idx {
        1 => Lang::ZhCn,
        2 => Lang::En,
        _ => Lang::ZhTw,
    };
    i18n::set_lang(lang);
}

pub fn print_banner() {
    let w = 48;
    let line = "─".repeat(w);
    println!();
    println!("{}", format!("┌{line}┐").bright_cyan());
    println!(
        "{}",
        format!("│{}│", pad_center("cursor-cleanup", w))
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        format!(
            "│{}│",
            pad_center(&format!("VSCode / Cursor  ·  v{VERSION}"), w)
        )
        .cyan()
    );
    println!(
        "{}",
        format!("│{}│", pad_center(t(Msg::BannerHint), w)).dimmed()
    );
    println!("{}", format!("└{line}┘").bright_cyan());
    println!();
}

pub fn ok(msg: &str) {
    println!("  {} {}", "✔".green().bold(), msg.green().bold());
}
pub fn warn(msg: &str) {
    println!("  {} {}", "⚠".yellow().bold(), msg.yellow());
}
pub fn dim(msg: &str) {
    println!("  {}", msg.dimmed());
}

pub fn pause_exit() {
    println!();
    print!("  {} ", t(Msg::PauseExit).bright_black());
    let _ = io::stdout().flush();
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s);
}

pub fn pause_menu() {
    println!();
    print!("  {} ", t(Msg::PressEnterMenu).bright_black());
    let _ = io::stdout().flush();
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s);
}

pub fn is_gui_launch() -> bool {
    console::Term::stdout().is_term() && std::env::args().len() <= 1
}

pub fn main_menu() -> usize {
    // Cancel / Esc → Exit
    ask(
        t(Msg::MainMenu),
        &[
            t(Msg::MenuClean),
            t(Msg::MenuBackup),
            t(Msg::MenuRestore),
            t(Msg::Exit),
        ],
        0,
    )
    .unwrap_or(3)
}

pub fn run_backup_flow() -> bool {
    loop {
        let Some(editors) = pick_editors_interactive(true) else {
            return false;
        };

        let mode = match ask(
            t(Msg::BackupMode),
            &[
                t(Msg::BackupConfigOnly),
                t(Msg::BackupConfigState),
                t(Msg::Back),
            ],
            1,
        ) {
            Some(m) => m,
            None => return false,
        };
        if mode == 2 {
            continue;
        }
        let include_state = mode == 1;

        // Empty → default timestamp folder name (e.g. cursor-backup-2026-…).
        dim(t(Msg::BackupNameHint));
        let custom_name = match Input::<String>::with_theme(&theme())
            .with_prompt(t(Msg::BackupNameAsk))
            .allow_empty(true)
            .interact_text()
        {
            Ok(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
            Err(_) => continue,
        };

        let mut did = false;
        for editor in editors {
            if !ensure_editor_ready(editor) {
                return did;
            }

            match backup::create_backup(editor, include_state, custom_name.as_deref()) {
                Ok(info) => {
                    ok(t(Msg::BackupDone));
                    dim(&format!(
                        "{}  ·  {} ({})",
                        info.name,
                        info.path.display(),
                        fmt_size(info.bytes)
                    ));
                    did = true;
                }
                Err(e) => warn(&format!("{}: {e}", t(Msg::BackupFail))),
            }
        }
        return did;
    }
}

pub fn run_restore_flow() -> bool {
    loop {
        let filter = match ask(
            t(Msg::PickApp),
            &[
                t(Msg::AppCursor),
                t(Msg::AppVscode),
                t(Msg::AppAll),
                t(Msg::Back),
            ],
            0,
        ) {
            Some(0) => Some(Editor::Cursor),
            Some(1) => Some(Editor::VsCode),
            Some(2) => None,
            _ => return false,
        };

        let mut did_any = false;
        // Action loop: Back returns to app picker (not main menu).
        loop {
            let backups = backup::list_backups(filter);
            if backups.is_empty() {
                warn(t(Msg::NoBackups));
                if let Some(root) = backup::backups_root() {
                    dim(&root.display().to_string());
                }
                break;
            }

            let action = match ask(
                t(Msg::BackupManage),
                &[
                    t(Msg::ActionRestore),
                    t(Msg::ActionDelete),
                    t(Msg::Back),
                ],
                0,
            ) {
                Some(a) => a,
                None => return did_any,
            };
            if action == 2 {
                break;
            }

            if action == 1 {
                match delete_backups_multi(&backups) {
                    DeletePick::Deleted => {
                        did_any = true;
                        // Stay here so the list refreshes; user can delete more or go Back.
                    }
                    DeletePick::Back => {}
                }
                continue;
            }

            let mut labels: Vec<String> = backups.iter().map(|b| b.label()).collect();
            labels.push(t(Msg::Back).to_string());
            let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();

            // Choosing a backup is the confirmation — restore immediately after.
            let idx = match ask(t(Msg::PickBackup), &label_refs, 0) {
                Some(i) => i,
                None => continue,
            };
            if idx >= backups.len() {
                continue;
            }
            let chosen: &BackupInfo = &backups[idx];
            let editor = match chosen.app.as_str() {
                "vscode" => Editor::VsCode,
                _ => Editor::Cursor,
            };

            println!();
            dim(&chosen.path.display().to_string());

            if !ensure_editor_ready(editor) {
                continue;
            }

            match backup::restore_backup(chosen) {
                Ok(()) => {
                    ok(t(Msg::RestoreDone));
                    dim(&chosen.path.display().to_string());
                    return true;
                }
                Err(e) => {
                    warn(&format!("{}: {e}", t(Msg::RestoreFail)));
                    continue;
                }
            }
        }

        if did_any {
            return true;
        }
    }
}

enum DeletePick {
    Deleted,
    Back,
}

/// Multi-select backups and delete them.
/// Last item is 「返回」; selecting it (or Esc / cancel) goes back without deleting.
fn delete_backups_multi(backups: &[BackupInfo]) -> DeletePick {
    if backups.is_empty() {
        return DeletePick::Back;
    }

    loop {
        let mut labels: Vec<String> = backups.iter().map(|b| b.label()).collect();
        labels.push(t(Msg::Back).to_string());
        let back_idx = backups.len();

        dim(t(Msg::DeleteMultiHint));
        let selected = match MultiSelect::with_theme(&theme())
            .with_prompt(t(Msg::PickDeleteBackups))
            .items(&labels)
            .interact()
        {
            Ok(s) => s,
            Err(_) => return DeletePick::Back,
        };

        // Explicit Back row, or Esc-style cancel with nothing useful selected.
        if selected.contains(&back_idx) {
            return DeletePick::Back;
        }
        if selected.is_empty() {
            dim(t(Msg::NothingToDelete));
            // Re-show list so user can pick Back or select items.
            continue;
        }

        let mut did = false;
        for idx in selected {
            let Some(info) = backups.get(idx) else {
                continue;
            };
            match backup::delete_backup(info) {
                Ok(()) => {
                    ok(&format!("{}  ·  {}", t(Msg::DeleteDone), info.label()));
                    did = true;
                }
                Err(e) => warn(&format!("{}: {} ({e})", t(Msg::DeleteFail), info.label())),
            }
        }
        return if did {
            DeletePick::Deleted
        } else {
            DeletePick::Back
        };
    }
}


pub fn pick_editors(cli_app: Option<&str>) -> Option<Vec<Editor>> {
    if let Some(app) = cli_app {
        return match app {
            "cursor" => Some(vec![Editor::Cursor]),
            "vscode" => Some(vec![Editor::VsCode]),
            "both" => Some(vec![Editor::Cursor, Editor::VsCode]),
            _ => {
                warn(t(Msg::BadApp));
                None
            }
        };
    }
    pick_editors_interactive(false)
}

/// `use_back`: backup/restore use 「返回」; cleanup uses 「退出」.
fn pick_editors_interactive(use_back: bool) -> Option<Vec<Editor>> {
    let last = if use_back {
        t(Msg::Back)
    } else {
        t(Msg::Exit)
    };
    match ask(
        t(Msg::PickApp),
        &[
            t(Msg::AppCursor),
            t(Msg::AppVscode),
            t(Msg::AppBoth),
            last,
        ],
        0,
    ) {
        Some(0) => Some(vec![Editor::Cursor]),
        Some(1) => Some(vec![Editor::VsCode]),
        Some(2) => Some(vec![Editor::Cursor, Editor::VsCode]),
        _ => None,
    }
}

pub fn print_scan(editor: Editor, root: &std::path::Path, total: u64, stats: &[TargetStat]) {
    println!();
    let title = format!("── {} ", editor.display_name());
    let fill = 40usize.saturating_sub(title.width());
    println!(
        "  {}",
        format!("{title}{}", "─".repeat(fill))
            .bright_magenta()
            .bold()
    );
    println!("  {}", root.display().to_string().dimmed());
    println!(
        "  {}  {}",
        t(Msg::TotalSize).dimmed(),
        fmt_size(total).bold().white()
    );
    println!();
    for (i, s) in stats.iter().enumerate() {
        let tag = match s.target.risk {
            Risk::Safe => i18n::risk_tag(Risk::Safe).green(),
            Risk::Medium => i18n::risk_tag(Risk::Medium).yellow(),
            Risk::High => i18n::risk_tag(Risk::High).red().bold(),
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
        t(Msg::Cleanable).dimmed(),
        fmt_size(cleanable).bold().bright_yellow()
    );
    println!();
}

fn sum_risk(stats: &[TargetStat], pred: impl Fn(Risk) -> bool) -> u64 {
    stats
        .iter()
        .filter(|s| pred(s.target.risk))
        .map(|s| s.bytes)
        .sum()
}

fn idx_risk(stats: &[TargetStat], pred: impl Fn(Risk) -> bool) -> Vec<usize> {
    stats
        .iter()
        .enumerate()
        .filter(|(_, s)| pred(s.target.risk))
        .map(|(i, _)| i)
        .collect()
}

pub enum EditorOutcome {
    Done,
    BackToApps,
}

enum Plan {
    Selected(Vec<usize>),
    Back,
    Skip,
}

fn pick_plan(stats: &[TargetStat], yes_all: bool) -> Plan {
    if yes_all {
        return Plan::Selected(idx_risk(stats, |r| matches!(r, Risk::Safe)));
    }

    loop {
        let safe = sum_risk(stats, |r| matches!(r, Risk::Safe));
        let std = sum_risk(stats, |r| matches!(r, Risk::Safe | Risk::Medium));
        let deep: u64 = stats.iter().map(|s| s.bytes).sum();

        let a = i18n::plan_conservative(&fmt_size(safe));
        let b = i18n::plan_standard(&fmt_size(std));
        let c = i18n::plan_deep(&fmt_size(deep));
        let items = [
            a.as_str(),
            b.as_str(),
            c.as_str(),
            t(Msg::PlanCustom),
            t(Msg::PlanSkipApp),
            t(Msg::Back),
        ];

        // Default: Standard — one Enter starts clean (no backup / per-item spam).
        match ask(t(Msg::PickPlan), &items, 1) {
            Some(0) => return Plan::Selected(idx_risk(stats, |r| matches!(r, Risk::Safe))),
            Some(1) => {
                return Plan::Selected(idx_risk(stats, |r| matches!(r, Risk::Safe | Risk::Medium)))
            }
            Some(2) => return Plan::Selected((0..stats.len()).collect()),
            Some(3) => {
                if let Some(sel) = pick_custom(stats) {
                    return Plan::Selected(sel);
                }
                continue;
            }
            Some(4) => return Plan::Skip,
            _ => return Plan::Back,
        }
    }
}

fn pick_custom(stats: &[TargetStat]) -> Option<Vec<usize>> {
    dim(t(Msg::CustomHint));
    let mut selected = Vec::new();
    let mut i = 0;
    while i < stats.len() {
        let s = &stats[i];
        let title = i18n::target_title(s.target.kind);
        let prompt = format!(
            "[{}/{}] {}（{} · {}）",
            i + 1,
            stats.len(),
            title,
            fmt_size(s.bytes),
            i18n::risk_word(s.target.risk)
        );
        let default = if matches!(s.target.risk, Risk::Safe) && s.bytes > 0 {
            0
        } else {
            1
        };
        match ask(&prompt, &[t(Msg::Clean), t(Msg::Skip), t(Msg::Back)], default) {
            Some(0) => {
                selected.push(i);
                i += 1;
            }
            Some(1) => i += 1,
            _ => return None,
        }
    }
    Some(selected)
}

pub fn run_editor(editor: Editor, yes_all: bool) -> EditorOutcome {
    if yes_all {
        run_once(editor, true);
        return EditorOutcome::Done;
    }

    loop {
        let Some((root, total, stats)) = collect_stats(editor) else {
            warn(&i18n::fmt_no_resolve(editor.display_name()));
            return EditorOutcome::Done;
        };
        if !root.exists() {
            warn(&i18n::fmt_no_dir(
                editor.display_name(),
                &root.display().to_string(),
            ));
            return EditorOutcome::Done;
        }

        print_scan(editor, &root, total, &stats);

        if !ensure_editor_ready(editor) {
            return EditorOutcome::BackToApps;
        }

        let selected = match pick_plan(&stats, false) {
            Plan::Back => return EditorOutcome::BackToApps,
            Plan::Skip => return EditorOutcome::Done,
            Plan::Selected(s) if s.is_empty() => {
                dim(t(Msg::NothingSelected));
                continue;
            }
            Plan::Selected(s) => s,
        };

        println!();
        dim(t(Msg::AboutToClean));
        for &idx in &selected {
            println!(
                "    · {}  ({})",
                i18n::target_title(stats[idx].target.kind),
                fmt_size(stats[idx].bytes)
            );
        }
        println!();

        // Plan selection is the confirmation — no second "are you sure?" / backup prompts.
        if execute_clean(editor, &root, total, &stats, &selected) {
            return EditorOutcome::Done;
        }
        continue;
    }
}

fn run_once(editor: Editor, yes_all: bool) {
    let Some((root, total, stats)) = collect_stats(editor) else {
        return;
    };
    if !root.exists() {
        return;
    }
    print_scan(editor, &root, total, &stats);

    // Non-interactive mode must not delete while the editor holds locks.
    if is_editor_running(editor) {
        warn(t(Msg::EditorRunningAbort));
        return;
    }

    let Plan::Selected(selected) = pick_plan(&stats, yes_all) else {
        return;
    };
    if selected.is_empty() {
        return;
    }
    let _ = execute_clean(editor, &root, total, &stats, &selected);
}

/// Cleanup never asks for backup mid-run. Use the Backup menu first if needed.
fn execute_clean(
    editor: Editor,
    root: &std::path::Path,
    total: u64,
    stats: &[TargetStat],
    selected: &[usize],
) -> bool {
    let mut freed_total = 0u64;

    for &idx in selected {
        let target = &stats[idx].target;
        let title = i18n::target_title(target.kind);

        print!("  {} {} ... ", "→".cyan(), title);
        let _ = io::stdout().flush();
        let result = run_target(root, target);
        freed_total += result.freed;
        if result.errors == 0 {
            println!("{} {}", t(Msg::Done).green().bold(), fmt_size(result.freed));
        } else {
            println!(
                "{}",
                i18n::fmt_partial(&fmt_size(result.freed), result.errors)
                    .yellow()
                    .bold()
            );
        }
    }

    let after = path_size(root);
    println!();
    println!("  {}", "────────────────────────────────".bright_green());
    ok(&i18n::fmt_summary(
        editor.display_name(),
        &fmt_size(total),
        &fmt_size(after),
        &fmt_size(freed_total),
    ));
    println!("  {}", "────────────────────────────────".bright_green());
    true
}
