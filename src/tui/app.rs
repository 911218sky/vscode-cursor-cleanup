use crate::backup::{self, sanitize_backup_name, BackupInfo};
use crate::cleanup::{
    collect_stats, force_quit_editor, is_editor_running, launch_editor, run_target,
    wait_until_data_unlocked, TargetStat,
};
use crate::fsutil::{fmt_size, path_size};
use crate::i18n::{self, t, Lang, Msg};
use crate::paths::{plan_target_indices, Editor, Risk};
use crate::tui::widgets::{
    checkbox_items, draw_banner, draw_confirm, draw_input_field, draw_progress, draw_scan_table,
    draw_scanning, icons_for_screen, list_items, menu_list, mouse_to_index, status_line,
    MenuScreen, StatusKind,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{ListState, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;
use tui_input::Input;
use tui_input::InputRequest;

#[derive(Clone)]
pub enum StatusMsg {
    None,
    Ok(String),
    Warn(String),
    Dim(String),
}

/// Work to resume after the user confirms force-quitting Cursor.
#[derive(Clone)]
enum PendingWork {
    ExecuteClean {
        editor: Editor,
        root: PathBuf,
        total: u64,
        stats: Vec<TargetStat>,
        selected: Vec<usize>,
    },
    Restore(BackupInfo),
    Backup {
        include_state: bool,
        custom_name: Option<String>,
    },
}

#[derive(Clone)]
enum ConfirmAction {
    DeleteBackups(Vec<BackupInfo>),
    Restore(BackupInfo),
    /// Extra confirm before cleaning High-risk targets (state.vscdb).
    Clean {
        editor: Editor,
        root: PathBuf,
        total: u64,
        stats: Vec<TargetStat>,
        selected: Vec<usize>,
    },
    ForceQuit {
        editor: Editor,
        then: Box<PendingWork>,
    },
}

#[derive(Clone)]
enum Screen {
    Language,
    MainMenu,
    ScanAndPlan {
        editor: Editor,
        root: PathBuf,
        total: u64,
        stats: Vec<TargetStat>,
        /// Cached once — avoid rebuilding plan strings every frame.
        plan_labels: Vec<String>,
    },
    CustomClean {
        editor: Editor,
        root: PathBuf,
        total: u64,
        stats: Vec<TargetStat>,
        idx: usize,
        selected: Vec<usize>,
    },
    BackupMode,
    BackupName {
        include_state: bool,
    },
    RestoreManage {
        backups: Vec<BackupInfo>,
    },
    RestorePick {
        backups: Vec<BackupInfo>,
    },
    DeleteMulti {
        backups: Vec<BackupInfo>,
    },
    Confirm {
        prompt: String,
        action: ConfirmAction,
        return_to: Box<Screen>,
    },
    /// Shown for one frame before the blocking force-quit so the UI is not silent.
    ForceQuitting {
        editor: Editor,
        then: Box<PendingWork>,
        return_to: Box<Screen>,
    },
    Scanning {
        editor: Editor,
    },
    Progress {
        editor: Editor,
        root: PathBuf,
        total: u64,
        stats: Vec<TargetStat>,
        selected: Vec<usize>,
        step: usize,
        freed_total: u64,
        log: Vec<String>,
        /// True when any clean step reported errors / residual bytes.
        any_errors: bool,
        summary_done: bool,
        finish_at: u64,
    },
}

pub struct App {
    screen: Screen,
    list_state: ListState,
    checkbox: Vec<bool>,
    name_input: Input,
    status: StatusMsg,
    pub should_quit: bool,
    pub did_work: bool,
    /// Set when the user confirmed force-quit; relaunch Cursor after successful work.
    relaunch_after: bool,
    list_hit_area: Rect,
    frame: u64,
    scan_at: u64,
}

impl App {
    pub fn new(skip_lang: bool) -> Self {
        let screen = if skip_lang {
            Screen::MainMenu
        } else {
            Screen::Language
        };
        Self {
            screen,
            list_state: ListState::default().with_selected(Some(0)),
            checkbox: Vec::new(),
            name_input: Input::default(),
            status: StatusMsg::None,
            should_quit: false,
            did_work: false,
            relaunch_after: false,
            list_hit_area: Rect::default(),
            frame: 0,
            scan_at: 0,
        }
    }

    pub fn tick(&mut self) {
        self.frame += 1;
        self.tick_force_quitting();
        self.tick_scanning();
        self.tick_progress();
    }

    /// Input screens should not idle-redraw — avoids flicker while typing.
    pub fn idle_redraw(&self) -> bool {
        !matches!(self.screen, Screen::BackupName { .. })
    }

    pub fn wants_animation(&self) -> bool {
        matches!(
            self.screen,
            Screen::Scanning { .. }
                | Screen::Progress { .. }
                | Screen::ForceQuitting { .. }
        )
    }

    fn tick_force_quitting(&mut self) {
        let Screen::ForceQuitting {
            editor,
            then,
            return_to,
        } = self.screen.clone()
        else {
            return;
        };
        if self.do_force_quit(editor) {
            self.continue_pending(*then);
        } else {
            self.screen = *return_to;
            let len = self.current_labels().len().max(1);
            self.reset_list(len);
        }
    }

    fn tick_scanning(&mut self) {
        let Screen::Scanning { editor } = self.screen.clone() else {
            return;
        };
        if self.frame < self.scan_at {
            return;
        }
        if let Some((root, total, stats)) = collect_stats(editor) {
            if root.exists() {
                self.screen = Self::scan_screen(editor, root, total, stats);
                self.reset_list(5);
                return;
            }
            self.status = StatusMsg::Warn(i18n::fmt_no_dir(
                editor.display_name(),
                &root.display().to_string(),
            ));
        } else {
            self.status = StatusMsg::Warn(i18n::fmt_no_resolve(editor.display_name()));
        }
        self.screen = Screen::MainMenu;
        self.reset_list(4);
    }

    fn tick_progress(&mut self) {
        let Screen::Progress {
            editor,
            root,
            total,
            stats,
            selected,
            step,
            freed_total,
            log,
            any_errors,
            summary_done,
            finish_at,
            ..
        } = self.screen.clone()
        else {
            return;
        };

        if !summary_done {
            if step < selected.len() {
                let idx = selected[step];
                let target = &stats[idx].target;
                let title = i18n::target_title(target.kind);
                let result = run_target(&root, target);
                let new_freed = freed_total + result.freed;
                let step_err = result.errors > 0;
                let line = if !step_err {
                    format!("✔ {title}  {}", fmt_size(result.freed))
                } else if result.remaining > 0 {
                    i18n::fmt_clean_residual(
                        &fmt_size(result.freed),
                        &fmt_size(result.remaining),
                    )
                } else {
                    i18n::fmt_partial(&fmt_size(result.freed), result.errors)
                };
                let mut new_log = log;
                new_log.push(line);
                self.screen = Screen::Progress {
                    editor,
                    root,
                    total,
                    stats,
                    selected,
                    step: step + 1,
                    freed_total: new_freed,
                    log: new_log,
                    any_errors: any_errors || step_err,
                    summary_done: false,
                    finish_at: 0,
                };
            } else {
                let after = path_size(&root);
                let mut new_log = log;
                new_log.push(i18n::fmt_summary(
                    editor.display_name(),
                    &fmt_size(total),
                    &fmt_size(after),
                    &fmt_size(freed_total),
                ));
                self.did_work = true;
                if any_errors {
                    self.relaunch_after = false;
                    self.status = StatusMsg::Warn(t(Msg::Partial).to_string());
                } else {
                    self.status = StatusMsg::Ok(t(Msg::AllDone).to_string());
                }
                self.screen = Screen::Progress {
                    editor,
                    root,
                    total,
                    stats,
                    selected,
                    step,
                    freed_total,
                    log: new_log,
                    any_errors,
                    summary_done: true,
                    finish_at: self.frame + 15,
                };
            }
            return;
        }

        if self.frame >= finish_at {
            if !any_errors {
                self.maybe_relaunch();
            } else {
                self.relaunch_after = false;
            }
            self.screen = Screen::MainMenu;
            self.reset_list(4);
        }
    }

    fn plan_labels_for(stats: &[TargetStat]) -> Vec<String> {
        let safe: u64 = stats
            .iter()
            .filter(|s| matches!(s.target.risk, Risk::Safe))
            .map(|s| s.bytes)
            .sum();
        let standard: u64 = stats
            .iter()
            .filter(|s| matches!(s.target.risk, Risk::Safe | Risk::Medium))
            .map(|s| s.bytes)
            .sum();
        let deep: u64 = stats.iter().map(|s| s.bytes).sum();
        vec![
            i18n::plan_conservative(&fmt_size(safe)),
            i18n::plan_standard(&fmt_size(standard)),
            i18n::plan_deep(&fmt_size(deep)),
            t(Msg::PlanCustom).to_string(),
            t(Msg::Back).to_string(),
        ]
    }

    fn selection_includes_high(stats: &[TargetStat], selected: &[usize]) -> bool {
        selected.iter().any(|&i| {
            stats
                .get(i)
                .map(|s| matches!(s.target.risk, Risk::High))
                .unwrap_or(false)
        })
    }

    /// Start a clean after optional deep-confirm and force-quit confirm.
    fn begin_clean(
        &mut self,
        editor: Editor,
        root: PathBuf,
        total: u64,
        stats: Vec<TargetStat>,
        selected: Vec<usize>,
    ) {
        if Self::selection_includes_high(&stats, &selected) {
            let return_to = self.screen.clone();
            self.screen = Screen::Confirm {
                prompt: t(Msg::ConfirmDeepClean).to_string(),
                action: ConfirmAction::Clean {
                    editor,
                    root,
                    total,
                    stats,
                    selected,
                },
                return_to: Box::new(return_to),
            };
            self.reset_list(2);
            return;
        }
        self.proceed_clean(editor, root, total, stats, selected);
    }

    fn proceed_clean(
        &mut self,
        editor: Editor,
        root: PathBuf,
        total: u64,
        stats: Vec<TargetStat>,
        selected: Vec<usize>,
    ) {
        let then = PendingWork::ExecuteClean {
            editor,
            root: root.clone(),
            total,
            stats: stats.clone(),
            selected: selected.clone(),
        };
        if self.ensure_editor_or_confirm(editor, then) {
            self.execute_clean(editor, root, total, stats, selected);
        }
    }

    fn scan_screen(
        editor: Editor,
        root: PathBuf,
        total: u64,
        stats: Vec<TargetStat>,
    ) -> Screen {
        let plan_labels = Self::plan_labels_for(&stats);
        Screen::ScanAndPlan {
            editor,
            root,
            total,
            stats,
            plan_labels,
        }
    }

    fn reset_list(&mut self, len: usize) {
        self.list_state = ListState::default();
        if len > 0 {
            self.list_state.select(Some(0));
        }
    }

    fn list_selected(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    fn list_up(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let cur = self.list_selected();
        if cur > 0 {
            self.list_state.select(Some(cur - 1));
        }
    }

    fn list_down(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let cur = self.list_selected();
        if cur + 1 < len {
            self.list_state.select(Some(cur + 1));
        }
    }

    fn current_labels(&self) -> Vec<String> {
        match &self.screen {
            Screen::Language => vec![
                t(Msg::LangTw).to_string(),
                t(Msg::LangCn).to_string(),
                t(Msg::LangEn).to_string(),
            ],
            Screen::MainMenu => vec![
                t(Msg::MenuClean).to_string(),
                t(Msg::MenuBackup).to_string(),
                t(Msg::MenuRestore).to_string(),
                t(Msg::Exit).to_string(),
            ],
            Screen::ScanAndPlan { plan_labels, .. } => plan_labels.clone(),
            Screen::CustomClean { stats, idx, .. } => {
                if *idx >= stats.len() {
                    return vec![];
                }
                let s = &stats[*idx];
                let _title = i18n::target_title(s.target.kind);
                vec![
                    t(Msg::Clean).to_string(),
                    t(Msg::Skip).to_string(),
                    t(Msg::Back).to_string(),
                ]
            }
            Screen::BackupMode => vec![
                t(Msg::BackupConfigOnly).to_string(),
                t(Msg::BackupConfigState).to_string(),
                t(Msg::Back).to_string(),
            ],
            Screen::RestoreManage { .. } => vec![
                t(Msg::ActionRestore).to_string(),
                t(Msg::ActionDelete).to_string(),
                t(Msg::Back).to_string(),
            ],
            Screen::RestorePick { backups, .. } => {
                let mut v: Vec<String> = backups.iter().map(|b| b.label()).collect();
                v.push(t(Msg::Back).to_string());
                v
            }
            Screen::DeleteMulti { backups, .. } => {
                let mut v: Vec<String> = backups.iter().map(|b| b.label()).collect();
                v.push(t(Msg::Back).to_string());
                v
            }
            Screen::Confirm { .. } => {
                vec![t(Msg::ConfirmYes).to_string(), t(Msg::ConfirmNo).to_string()]
            }
            _ => vec![],
        }
    }

    /// If the editor is not running, returns true so the caller can proceed.
    /// If it is running, shows a confirm dialog and returns false (caller must wait).
    fn ensure_editor_or_confirm(&mut self, editor: Editor, then: PendingWork) -> bool {
        if !is_editor_running(editor) {
            return true;
        }
        let return_to = self.screen.clone();
        self.status = StatusMsg::Warn(i18n::fmt_running(editor.display_name()));
        self.screen = Screen::Confirm {
            prompt: format!("{} {}", editor.display_name(), t(Msg::ConfirmForceQuit)),
            action: ConfirmAction::ForceQuit {
                editor,
                then: Box::new(then),
            },
            return_to: Box::new(return_to),
        };
        self.reset_list(2);
        false
    }

    fn do_force_quit(&mut self, editor: Editor) -> bool {
        let ok = force_quit_editor(editor, true);
        if ok {
            self.status = StatusMsg::Ok(format!(
                "{} {}",
                t(Msg::ForceQuitOk),
                editor.display_name()
            ));
        } else {
            self.relaunch_after = false;
            self.status = StatusMsg::Warn(t(Msg::ForceQuitFail).to_string());
        }
        ok
    }

    /// After a successful clean/restore/backup that followed force-quit, relaunch Cursor.
    fn maybe_relaunch(&mut self) {
        if !self.relaunch_after {
            return;
        }
        self.relaunch_after = false;
        if launch_editor(Editor::Cursor) {
            let suffix = t(Msg::RelaunchOk);
            self.status = match std::mem::replace(&mut self.status, StatusMsg::None) {
                StatusMsg::Ok(s) => StatusMsg::Ok(format!("{s} — {suffix}")),
                other => {
                    let _ = other;
                    StatusMsg::Ok(suffix.to_string())
                }
            };
        } else {
            let suffix = t(Msg::RelaunchFail);
            self.status = match std::mem::replace(&mut self.status, StatusMsg::None) {
                StatusMsg::Ok(s) => StatusMsg::Warn(format!("{s} — {suffix}")),
                StatusMsg::Warn(s) => StatusMsg::Warn(format!("{s} — {suffix}")),
                _ => StatusMsg::Warn(suffix.to_string()),
            };
        }
    }

    fn continue_pending(&mut self, work: PendingWork) {
        match work {
            PendingWork::ExecuteClean {
                editor,
                root,
                total,
                stats,
                selected,
            } => {
                self.execute_clean(editor, root, total, stats, selected);
            }
            PendingWork::Restore(info) => {
                self.do_restore(info);
            }
            PendingWork::Backup {
                include_state,
                custom_name,
            } => {
                // Already force-quit — do not re-enter ensure_editor_or_confirm.
                self.run_backup_now(include_state, custom_name);
            }
        }
    }

    /// Entry point for restore — always checks / confirms force-quit first.
    fn start_restore(&mut self, info: BackupInfo) {
        if !self.ensure_editor_or_confirm(
            Editor::Cursor,
            PendingWork::Restore(info.clone()),
        ) {
            return;
        }
        self.do_restore(info);
    }

    fn do_restore(&mut self, info: BackupInfo) {
        if is_editor_running(Editor::Cursor) {
            self.relaunch_after = false;
            self.status = StatusMsg::Warn(t(Msg::RestoreEditorRunning).to_string());
            self.screen = Screen::MainMenu;
            self.reset_list(4);
            return;
        }
        match backup::restore_backup(&info) {
            Ok(()) => {
                self.did_work = true;
                self.status = StatusMsg::Ok(format!(
                    "{} — {}",
                    t(Msg::RestoreDone),
                    t(Msg::RestoreVerified)
                ));
                self.maybe_relaunch();
                self.screen = Screen::MainMenu;
                self.reset_list(4);
            }
            Err(e) => {
                self.relaunch_after = false;
                let es = e.to_string();
                let msg = if es.contains("cursor still running") {
                    t(Msg::RestoreEditorRunning).to_string()
                } else if es.contains("data still locked") {
                    t(Msg::DataStillLocked).to_string()
                } else if es.contains("state sidecars still present") {
                    format!("{}: WAL/SHM still locked", t(Msg::RestoreFail))
                } else if es.contains("restore verify failed") || es.contains("size mismatch") {
                    format!("{}: size mismatch", t(Msg::RestoreFail))
                } else {
                    format!("{}: {e}", t(Msg::RestoreFail))
                };
                self.status = StatusMsg::Warn(msg);
                self.screen = Screen::MainMenu;
                self.reset_list(4);
            }
        }
    }

    fn run_backup(&mut self, include_state: bool, custom_name: Option<String>) {
        let editor = Editor::Cursor;
        if !self.ensure_editor_or_confirm(
            editor,
            PendingWork::Backup {
                include_state,
                custom_name: custom_name.clone(),
            },
        ) {
            return;
        }
        self.run_backup_now(include_state, custom_name);
    }

    /// Perform backup without re-checking whether the editor is running.
    fn run_backup_now(&mut self, include_state: bool, custom_name: Option<String>) {
        let editor = Editor::Cursor;
        match backup::create_backup(editor, include_state, custom_name.as_deref()) {
            Ok(info) => {
                self.did_work = true;
                self.status = StatusMsg::Ok(format!(
                    "{} — {} ({})",
                    t(Msg::BackupDone),
                    info.name,
                    fmt_size(info.bytes)
                ));
                self.maybe_relaunch();
            }
            Err(e) => {
                self.relaunch_after = false;
                self.status = StatusMsg::Warn(format!("{}: {e}", t(Msg::BackupFail)));
            }
        }
        self.screen = Screen::MainMenu;
        self.reset_list(4);
    }

    fn start_clean(&mut self) {
        self.screen = Screen::Scanning {
            editor: Editor::Cursor,
        };
        self.scan_at = self.frame + 6;
    }

    /// 0=conservative, 1=standard (safe+history), 2=deep, 3=custom → None
    fn plan_indices(&self, choice: usize, stats: &[TargetStat]) -> Option<Vec<usize>> {
        let risks: Vec<Risk> = stats.iter().map(|s| s.target.risk).collect();
        plan_target_indices(choice, &risks)
    }

    fn open_restore_manage(&mut self) {
        let backups = backup::list_backups(Some(Editor::Cursor));
        if backups.is_empty() {
            self.status = StatusMsg::Warn(t(Msg::NoBackups).to_string());
            self.screen = Screen::MainMenu;
            self.reset_list(4);
        } else {
            self.screen = Screen::RestoreManage { backups };
            self.reset_list(3);
        }
    }

    fn execute_clean(
        &mut self,
        editor: Editor,
        root: PathBuf,
        total: u64,
        stats: Vec<TargetStat>,
        selected: Vec<usize>,
    ) {
        // Process may already be gone without force-quit — still wait for locks.
        if !wait_until_data_unlocked(editor, true) {
            self.relaunch_after = false;
            self.status = StatusMsg::Warn(t(Msg::DataStillLocked).to_string());
            self.screen = Screen::MainMenu;
            self.reset_list(4);
            return;
        }
        self.screen = Screen::Progress {
            editor,
            root,
            total,
            stats,
            selected,
            step: 0,
            freed_total: 0,
            log: vec![t(Msg::AboutToClean).to_string()],
            any_errors: false,
            summary_done: false,
            finish_at: 0,
        };
    }

    fn menu_screen(&self) -> MenuScreen {
        match &self.screen {
            Screen::Language => MenuScreen::Language,
            Screen::MainMenu => MenuScreen::MainMenu,
            Screen::ScanAndPlan { .. } => MenuScreen::Plan,
            Screen::CustomClean { .. } => MenuScreen::CustomClean,
            Screen::BackupMode => MenuScreen::BackupMode,
            Screen::RestoreManage { .. } => MenuScreen::RestoreManage,
            Screen::Confirm { .. } => MenuScreen::Confirm,
            _ => MenuScreen::Generic,
        }
    }

    fn handle_list_select(&mut self, idx: usize) {
        match &self.screen {
            Screen::Language => {
                let lang = match idx {
                    1 => Lang::ZhCn,
                    2 => Lang::En,
                    _ => Lang::ZhTw,
                };
                i18n::set_lang(lang);
                self.screen = Screen::MainMenu;
                self.reset_list(4);
            }
            Screen::MainMenu => match idx {
                0 => self.start_clean(),
                1 => {
                    self.screen = Screen::BackupMode;
                    self.reset_list(3);
                }
                2 => self.open_restore_manage(),
                _ => self.should_quit = true,
            },
            Screen::ScanAndPlan {
                editor,
                root,
                total,
                stats,
                ..
            } => {
                let editor = *editor;
                let root = root.clone();
                let total = *total;
                let stats = stats.clone();
                // Labels: 0 conservative, 1 standard, 2 deep, 3 custom, 4 back
                if idx == 4 {
                    self.screen = Screen::MainMenu;
                    self.reset_list(4);
                    return;
                }
                if idx == 3 {
                    // Browse custom items without quitting; confirm quit when cleaning starts.
                    self.screen = Screen::CustomClean {
                        editor,
                        root,
                        total,
                        stats,
                        idx: 0,
                        selected: Vec::new(),
                    };
                    self.reset_list(3);
                    return;
                }
                if let Some(sel) = self.plan_indices(idx, &stats) {
                    if sel.is_empty() {
                        self.screen = Screen::MainMenu;
                        self.reset_list(4);
                    } else {
                        self.begin_clean(editor, root, total, stats, sel);
                    }
                }
            }
            Screen::CustomClean {
                editor,
                root,
                total,
                stats,
                idx: item_idx,
                selected,
            } => {
                let editor = *editor;
                let root = root.clone();
                let total = *total;
                let stats = stats.clone();
                let mut item_idx = *item_idx;
                let mut selected = selected.clone();
                match idx {
                    0 => {
                        selected.push(item_idx);
                        item_idx += 1;
                    }
                    1 => item_idx += 1,
                    _ => {
                        self.screen = Self::scan_screen(editor, root, total, stats);
                        self.reset_list(5);
                        return;
                    }
                }
                if item_idx >= stats.len() {
                    if selected.is_empty() {
                        self.status = StatusMsg::Dim(t(Msg::NothingSelected).to_string());
                        self.screen = Self::scan_screen(editor, root, total, stats);
                        self.reset_list(5);
                    } else {
                        // Return to plan on cancel — not the last custom item
                        // (re-confirming Clean there would duplicate the last index).
                        self.screen = Self::scan_screen(editor, root.clone(), total, stats.clone());
                        self.reset_list(5);
                        self.begin_clean(editor, root, total, stats, selected);
                    }
                } else {
                    let default = if matches!(stats[item_idx].target.risk, Risk::Safe)
                        && stats[item_idx].bytes > 0
                    {
                        0
                    } else {
                        1
                    };
                    self.screen = Screen::CustomClean {
                        editor,
                        root,
                        total,
                        stats,
                        idx: item_idx,
                        selected,
                    };
                    self.reset_list(3);
                    self.list_state.select(Some(default));
                }
            }
            Screen::BackupMode => {
                if idx == 2 {
                    self.screen = Screen::MainMenu;
                    self.reset_list(4);
                    return;
                }
                let include_state = idx == 1;
                self.screen = Screen::BackupName { include_state };
                self.name_input = Input::default();
            }
            Screen::RestoreManage { backups } => {
                let backups = backups.clone();
                match idx {
                    0 => {
                        let n = backups.len() + 1;
                        self.screen = Screen::RestorePick { backups };
                        self.reset_list(n);
                    }
                    1 => {
                        let n = backups.len() + 1;
                        self.checkbox = vec![false; n];
                        self.screen = Screen::DeleteMulti { backups };
                        self.reset_list(n);
                    }
                    _ => {
                        self.screen = Screen::MainMenu;
                        self.reset_list(4);
                    }
                }
            }
            Screen::RestorePick { backups } => {
                if idx >= backups.len() {
                    self.screen = Screen::RestoreManage {
                        backups: backups.clone(),
                    };
                    self.reset_list(3);
                    return;
                }
                let chosen = backups[idx].clone();
                let return_to = self.screen.clone();
                let mut prompt = format!("{}\n{}", t(Msg::ConfirmRestore), chosen.label());
                if is_editor_running(Editor::Cursor) {
                    prompt.push_str(&format!("\n{}", t(Msg::ConfirmRestoreRunning)));
                }
                self.screen = Screen::Confirm {
                    prompt,
                    action: ConfirmAction::Restore(chosen),
                    return_to: Box::new(return_to),
                };
                self.reset_list(2);
            }
            Screen::DeleteMulti { backups } => {
                if idx >= backups.len() {
                    self.screen = Screen::RestoreManage {
                        backups: backups.clone(),
                    };
                    self.reset_list(3);
                    return;
                }
                if idx < self.checkbox.len() {
                    self.checkbox[idx] = !self.checkbox[idx];
                }
            }
            Screen::Confirm { action, return_to, .. } => {
                let action = action.clone();
                let return_to = return_to.clone();
                if idx == 1 {
                    // Cancel force-quit / other confirms — never leave relaunch sticky.
                    self.relaunch_after = false;
                    self.screen = *return_to;
                    let len = self.current_labels().len().max(1);
                    self.reset_list(len);
                    return;
                }
                match action {
                    ConfirmAction::Restore(info) => {
                        self.start_restore(info);
                    }
                    ConfirmAction::DeleteBackups(infos) => {
                        for info in &infos {
                            match backup::delete_backup(info) {
                                Ok(()) => {
                                    self.did_work = true;
                                    self.status = StatusMsg::Ok(format!(
                                        "{} — {} · {}",
                                        t(Msg::DeleteDone),
                                        t(Msg::DeleteVerified),
                                        info.label()
                                    ));
                                }
                                Err(e) => {
                                    self.status = StatusMsg::Warn(format!(
                                        "{}: {} ({e})",
                                        t(Msg::DeleteFail),
                                        info.label()
                                    ));
                                }
                            }
                        }
                        self.screen = Screen::MainMenu;
                        self.reset_list(4);
                    }
                    ConfirmAction::Clean {
                        editor,
                        root,
                        total,
                        stats,
                        selected,
                    } => {
                        self.proceed_clean(editor, root, total, stats, selected);
                    }
                    ConfirmAction::ForceQuit { editor, then } => {
                        // Draw once, then tick_force_quitting() performs the (blocking) kill.
                        self.relaunch_after = true;
                        self.status = StatusMsg::Warn(t(Msg::ForceQuitting).to_string());
                        self.screen = Screen::ForceQuitting {
                            editor,
                            then,
                            return_to,
                        };
                    }
                }
            }
            _ => {}
        }
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => self.handle_mouse(m.column, m.row),
                MouseEventKind::ScrollUp => {
                    let len = self.current_labels().len();
                    self.list_up(len);
                }
                MouseEventKind::ScrollDown => {
                    let len = self.current_labels().len();
                    self.list_down(len);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn handle_mouse(&mut self, col: u16, row: u16) {
        let labels_len = self.current_labels().len();
        if labels_len == 0 {
            return;
        }
        let list_area = self.list_hit_area;
        if list_area.width == 0 {
            return;
        }
        if let Some(idx) =
            mouse_to_index(list_area, col, row, labels_len, self.list_state.offset())
        {
            self.list_state.select(Some(idx));
            if matches!(
                self.screen,
                Screen::DeleteMulti { .. } | Screen::CustomClean { .. }
            ) {
                self.handle_list_select(idx);
            } else {
                self.handle_list_activate();
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }

        match &self.screen {
            // Ignore input while force-quit is in flight (Esc already blocked in handle_back).
            Screen::ForceQuitting { .. } => return,
            Screen::BackupName { .. } => self.handle_name_key(key),
            Screen::DeleteMulti { backups } => {
                let len = backups.len() + 1;
                match key.code {
                    KeyCode::Esc => {
                        self.screen = Screen::RestoreManage {
                            backups: backups.clone(),
                        };
                        self.reset_list(3);
                    }
                    KeyCode::Up | KeyCode::Char('k') => self.list_up(len),
                    KeyCode::Down | KeyCode::Char('j') => self.list_down(len),
                    KeyCode::Char(' ') => {
                        let sel = self.list_selected();
                        if sel < self.checkbox.len() {
                            self.checkbox[sel] = !self.checkbox[sel];
                        }
                    }
                    KeyCode::Enter => {
                        let back_idx = backups.len();
                        let selected: Vec<usize> = self
                            .checkbox
                            .iter()
                            .enumerate()
                            .filter(|(i, c)| **c && *i != back_idx)
                            .map(|(i, _)| i)
                            .collect();
                        if selected.is_empty()
                            || self.checkbox.get(back_idx).copied().unwrap_or(false)
                        {
                            self.screen = Screen::RestoreManage {
                                backups: backups.clone(),
                            };
                            self.reset_list(3);
                        } else {
                            let to_delete: Vec<BackupInfo> = selected
                                .iter()
                                .filter_map(|i| backups.get(*i).cloned())
                                .collect();
                            let return_to = self.screen.clone();
                            self.screen = Screen::Confirm {
                                prompt: format!(
                                    "{}\n{}",
                                    t(Msg::ConfirmDelete),
                                    to_delete
                                        .iter()
                                        .map(|b| b.label())
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                ),
                                action: ConfirmAction::DeleteBackups(to_delete),
                                return_to: Box::new(return_to),
                            };
                            self.reset_list(2);
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                let len = self.current_labels().len();
                match key.code {
                    KeyCode::Esc => self.handle_back(),
                    KeyCode::Up | KeyCode::Char('k') => self.list_up(len),
                    KeyCode::Down | KeyCode::Char('j') => self.list_down(len),
                    KeyCode::Enter => self.handle_list_activate(),
                    KeyCode::Char(' ') if matches!(self.screen, Screen::DeleteMulti { .. }) => {
                        let sel = self.list_selected();
                        if sel < self.checkbox.len() {
                            self.checkbox[sel] = !self.checkbox[sel];
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_name_key(&mut self, key: KeyEvent) {
        let Screen::BackupName { include_state } = self.screen.clone() else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::BackupMode;
                self.reset_list(3);
            }
            KeyCode::Enter => {
                let raw = self.name_input.value().trim();
                let custom = if raw.is_empty() {
                    None
                } else if sanitize_backup_name(raw).is_none() {
                    self.status = StatusMsg::Warn(t(Msg::BackupNameInvalid).to_string());
                    None
                } else {
                    Some(raw.to_string())
                };
                self.run_backup(include_state, custom);
            }
            KeyCode::Backspace => {
                self.name_input.handle(InputRequest::DeletePrevChar);
            }
            KeyCode::Delete => {
                self.name_input.handle(InputRequest::DeleteNextChar);
            }
            KeyCode::Left => {
                self.name_input.handle(InputRequest::GoToPrevChar);
            }
            KeyCode::Right => {
                self.name_input.handle(InputRequest::GoToNextChar);
            }
            KeyCode::Char(c) => {
                self.name_input.handle(InputRequest::InsertChar(c));
            }
            _ => {}
        }
    }

    fn handle_back(&mut self) {
        let next = match self.screen.clone() {
            Screen::Language | Screen::MainMenu => {
                self.should_quit = true;
                return;
            }
            // Don't abort mid force-quit or mid-clean (partial deletes).
            Screen::ForceQuitting { .. } => return,
            Screen::Progress { summary_done, .. } if !summary_done => return,
            Screen::Progress {
                summary_done: true,
                ..
            } => {
                // Leaving the post-clean summary early — do not sticky-relaunch later.
                self.relaunch_after = false;
                Screen::MainMenu
            }
            Screen::Scanning { .. } | Screen::ScanAndPlan { .. } | Screen::BackupMode => {
                Screen::MainMenu
            }
            Screen::CustomClean {
                editor,
                root,
                total,
                stats,
                ..
            } => Self::scan_screen(editor, root, total, stats),
            Screen::BackupName { .. } => return,
            Screen::RestoreManage { .. } => Screen::MainMenu,
            Screen::RestorePick { backups } | Screen::DeleteMulti { backups } => {
                Screen::RestoreManage { backups }
            }
            Screen::Confirm { return_to, .. } => {
                self.relaunch_after = false;
                *return_to.clone()
            }
            _ => Screen::MainMenu,
        };
        self.screen = next;
        let len = self.current_labels().len().max(1);
        self.reset_list(len);
    }

    fn handle_list_activate(&mut self) {
        if matches!(self.screen, Screen::DeleteMulti { .. }) {
            return;
        }
        let idx = self.list_selected();
        self.handle_list_select(idx);
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let frame = self.frame;
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(f.area());

        let animate_banner = !matches!(self.screen, Screen::BackupName { .. });
        draw_banner(f, chunks[0], t(Msg::MainMenu), frame, animate_banner);

        match &self.screen {
            Screen::Scanning { editor } => {
                self.list_hit_area = Rect::default();
                draw_scanning(f, chunks[1], *editor, frame);
            }
            Screen::ScanAndPlan {
                editor,
                root,
                total,
                stats,
                ..
            } => {
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                    .split(chunks[1]);
                draw_scan_table(
                    f,
                    cols[0],
                    *editor,
                    &root.display().to_string(),
                    *total,
                    stats,
                    frame,
                );
                let labels = self.current_labels();
                let sel = self.list_selected();
                let icons: Vec<&str> = icons_for_screen(MenuScreen::Plan, labels.len());
                let items = list_items(&labels, sel, &icons, frame);
                self.list_hit_area = cols[1];
                f.render_stateful_widget(
                    menu_list(items, sel, t(Msg::PickPlan), frame),
                    cols[1],
                    &mut self.list_state,
                );
            }
            Screen::CustomClean {
                stats,
                idx,
                ..
            } => {
                let s = &stats[*idx];
                let title = i18n::target_title(s.target.kind);
                let prompt = format!(
                    "[{}/{}] {}（{} · {}）",
                    idx + 1,
                    stats.len(),
                    title,
                    fmt_size(s.bytes),
                    i18n::risk_word(s.target.risk)
                );
                let labels = self.current_labels();
                let sel = self.list_selected();
                let icons: Vec<&str> =
                    icons_for_screen(MenuScreen::CustomClean, labels.len());
                let items = list_items(&labels, sel, &icons, frame);
                self.list_hit_area = chunks[1];
                f.render_stateful_widget(
                    menu_list(items, sel, &prompt, frame),
                    chunks[1],
                    &mut self.list_state,
                );
            }
            Screen::BackupName { .. } => {
                self.list_hit_area = Rect::default();
                let input_area = chunks[1];
                let scroll = self
                    .name_input
                    .visual_scroll(input_area.width.saturating_sub(4) as usize);
                let display = self
                    .name_input
                    .value()
                    .chars()
                    .skip(scroll)
                    .collect::<String>();
                let cursor = self.name_input.visual_cursor().saturating_sub(scroll);
                draw_input_field(
                    f,
                    input_area,
                    t(Msg::BackupNameHint),
                    &display,
                    cursor,
                );
            }
            Screen::DeleteMulti { .. } => {
                let labels = self.current_labels();
                let sel = self.list_selected();
                let items = checkbox_items(&labels, &self.checkbox, sel, frame);
                let hint = Paragraph::new(t(Msg::DeleteMultiHint))
                    .style(Style::default().fg(Color::DarkGray));
                let sub = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(2), Constraint::Min(5)])
                    .split(chunks[1]);
                f.render_widget(hint, sub[0]);
                self.list_hit_area = sub[1];
                f.render_stateful_widget(
                    menu_list(items, sel, t(Msg::PickDeleteBackups), frame),
                    sub[1],
                    &mut self.list_state,
                );
            }
            Screen::Progress {
                log,
                step,
                selected,
                ..
            } => {
                self.list_hit_area = Rect::default();
                draw_progress(f, chunks[1], log, *step, selected.len(), frame);
            }
            Screen::ForceQuitting { .. } => {
                self.list_hit_area = Rect::default();
                draw_confirm(f, chunks[1], t(Msg::ForceQuitting), frame);
            }
            Screen::Confirm { prompt, .. } => {
                let confirm_area = Rect {
                    height: chunks[1].height.saturating_sub(6),
                    ..chunks[1]
                };
                draw_confirm(f, confirm_area, prompt, frame);
                let labels = self.current_labels();
                let sel = self.list_selected();
                let icons: Vec<&str> = icons_for_screen(MenuScreen::Confirm, labels.len());
                let items = list_items(&labels, sel, &icons, frame);
                let list_area = Rect {
                    y: chunks[1].y + chunks[1].height.saturating_sub(5),
                    height: 5,
                    ..chunks[1]
                };
                self.list_hit_area = list_area;
                f.render_stateful_widget(
                    menu_list(items, sel, "", frame),
                    list_area,
                    &mut self.list_state,
                );
            }
            _ => {
                let labels = self.current_labels();
                let title = match &self.screen {
                    Screen::Language => t(Msg::PickLang),
                    Screen::MainMenu => t(Msg::MainMenu),
                    Screen::BackupMode => t(Msg::BackupMode),
                    Screen::RestoreManage { .. } => t(Msg::BackupManage),
                    Screen::RestorePick { .. } => t(Msg::PickBackup),
                    _ => "",
                };
                let sel = self.list_selected();
                let menu = self.menu_screen();
                let icons: Vec<&str> = icons_for_screen(menu, labels.len());
                let items = list_items(&labels, sel, &icons, frame);
                self.list_hit_area = chunks[1];
                f.render_stateful_widget(
                    menu_list(items, sel, title, frame),
                    chunks[1],
                    &mut self.list_state,
                );
            }
        }

        let status_widget = match &self.status {
            StatusMsg::None => status_line(t(Msg::MouseHint), StatusKind::Dim, frame),
            StatusMsg::Ok(s) => status_line(s, StatusKind::Ok, frame),
            StatusMsg::Warn(s) => status_line(s, StatusKind::Warn, frame),
            StatusMsg::Dim(s) => status_line(s, StatusKind::Dim, frame),
        };
        f.render_widget(status_widget, chunks[2]);
    }
}
