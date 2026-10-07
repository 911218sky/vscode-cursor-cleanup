use crate::backup::{self, sanitize_backup_name, BackupInfo};
use crate::cleanup::{
    collect_stats, force_quit_editor, is_editor_running, run_target, TargetStat,
};
use crate::fsutil::{fmt_size, path_size};
use crate::i18n::{self, t, Lang, Msg};
use crate::paths::{Editor, Risk};
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

#[derive(Clone)]
enum ConfirmAction {
    DeleteBackups(Vec<BackupInfo>),
    Restore(BackupInfo),
}

#[derive(Clone)]
enum Screen {
    Language,
    MainMenu,
    PickApp {
        use_back: bool,
        for_backup: bool,
    },
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
    BackupMode {
        editors: Vec<Editor>,
    },
    BackupName {
        editors: Vec<Editor>,
        include_state: bool,
    },
    RestoreFilter,
    RestoreManage {
        filter: Option<Editor>,
        backups: Vec<BackupInfo>,
    },
    RestorePick {
        filter: Option<Editor>,
        backups: Vec<BackupInfo>,
    },
    DeleteMulti {
        filter: Option<Editor>,
        backups: Vec<BackupInfo>,
    },
    Confirm {
        prompt: String,
        action: ConfirmAction,
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
    list_hit_area: Rect,
    pending_editors: Vec<Editor>,
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
            list_hit_area: Rect::default(),
            pending_editors: Vec::new(),
            frame: 0,
            scan_at: 0,
        }
    }

    pub fn tick(&mut self) {
        self.frame += 1;
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
            Screen::Scanning { .. } | Screen::Progress { .. }
        )
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
                self.reset_list(6);
                return;
            }
            self.status = StatusMsg::Warn(i18n::fmt_no_dir(
                editor.display_name(),
                &root.display().to_string(),
            ));
        } else {
            self.status = StatusMsg::Warn(i18n::fmt_no_resolve(editor.display_name()));
        }
        self.pending_editors.remove(0);
        self.advance_next_editor_clean();
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
                let line = if result.errors == 0 {
                    format!("✔ {title}  {}", fmt_size(result.freed))
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
                self.status = StatusMsg::Ok(t(Msg::AllDone).to_string());
                self.screen = Screen::Progress {
                    editor,
                    root,
                    total,
                    stats,
                    selected,
                    step,
                    freed_total,
                    log: new_log,
                    summary_done: true,
                    finish_at: self.frame + 15,
                };
            }
            return;
        }

        if self.frame >= finish_at {
            self.finish_editor_clean();
        }
    }

    fn plan_labels_for(stats: &[TargetStat]) -> Vec<String> {
        let safe: u64 = stats
            .iter()
            .filter(|s| matches!(s.target.risk, Risk::Safe))
            .map(|s| s.bytes)
            .sum();
        let std: u64 = stats
            .iter()
            .filter(|s| matches!(s.target.risk, Risk::Safe | Risk::Medium))
            .map(|s| s.bytes)
            .sum();
        let deep: u64 = stats.iter().map(|s| s.bytes).sum();
        vec![
            i18n::plan_conservative(&fmt_size(safe)),
            i18n::plan_standard(&fmt_size(std)),
            i18n::plan_deep(&fmt_size(deep)),
            t(Msg::PlanCustom).to_string(),
            t(Msg::PlanSkipApp).to_string(),
            t(Msg::Back).to_string(),
        ]
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
            Screen::PickApp { use_back, .. } => {
                let last = if *use_back {
                    t(Msg::Back)
                } else {
                    t(Msg::Exit)
                };
                vec![
                    t(Msg::AppCursor).to_string(),
                    t(Msg::AppVscode).to_string(),
                    t(Msg::AppBoth).to_string(),
                    last.to_string(),
                ]
            }
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
            Screen::BackupMode { .. } => vec![
                t(Msg::BackupConfigOnly).to_string(),
                t(Msg::BackupConfigState).to_string(),
                t(Msg::Back).to_string(),
            ],
            Screen::RestoreFilter => vec![
                t(Msg::AppCursor).to_string(),
                t(Msg::AppVscode).to_string(),
                t(Msg::AppAll).to_string(),
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

    fn ensure_editor(&mut self, editor: Editor) -> bool {
        if !is_editor_running(editor) {
            return true;
        }
        self.status = StatusMsg::Warn(i18n::fmt_running(editor.display_name()));
        let ok = force_quit_editor(editor, true);
        if ok {
            self.status = StatusMsg::Ok(format!(
                "{} {}",
                t(Msg::ForceQuitOk),
                editor.display_name()
            ));
            true
        } else {
            self.status = StatusMsg::Warn(t(Msg::ForceQuitFail).to_string());
            false
        }
    }

    fn start_clean_editors(&mut self, editors: Vec<Editor>) {
        self.pending_editors = editors;
        self.advance_next_editor_clean();
    }

    fn advance_next_editor_clean(&mut self) {
        let Some(editor) = self.pending_editors.first().copied() else {
            self.screen = Screen::MainMenu;
            self.reset_list(4);
            return;
        };
        self.screen = Screen::Scanning { editor };
        self.scan_at = self.frame + 6;
    }

    fn finish_editor_clean(&mut self) {
        if !self.pending_editors.is_empty() {
            self.pending_editors.remove(0);
        }
        if self.pending_editors.is_empty() {
            self.screen = Screen::MainMenu;
            self.reset_list(4);
        } else {
            self.advance_next_editor_clean();
        }
    }

    fn plan_indices(&self, choice: usize, stats: &[TargetStat]) -> Option<Vec<usize>> {
        match choice {
            0 => Some(
                stats
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| matches!(s.target.risk, Risk::Safe))
                    .map(|(i, _)| i)
                    .collect(),
            ),
            1 => Some(
                stats
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| matches!(s.target.risk, Risk::Safe | Risk::Medium))
                    .map(|(i, _)| i)
                    .collect(),
            ),
            2 => Some((0..stats.len()).collect()),
            3 => None, // custom
            4 => Some(vec![]), // skip app
            _ => None,
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
        self.screen = Screen::Progress {
            editor,
            root,
            total,
            stats,
            selected,
            step: 0,
            freed_total: 0,
            log: vec![t(Msg::AboutToClean).to_string()],
            summary_done: false,
            finish_at: 0,
        };
    }

    fn menu_screen(&self) -> MenuScreen {
        match &self.screen {
            Screen::Language => MenuScreen::Language,
            Screen::MainMenu => MenuScreen::MainMenu,
            Screen::PickApp { .. } => MenuScreen::PickApp,
            Screen::ScanAndPlan { .. } => MenuScreen::Plan,
            Screen::CustomClean { .. } => MenuScreen::CustomClean,
            Screen::BackupMode { .. } => MenuScreen::BackupMode,
            Screen::RestoreFilter => MenuScreen::RestoreFilter,
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
                0 => {
                    self.screen = Screen::PickApp {
                        use_back: false,
                        for_backup: false,
                    };
                    self.reset_list(4);
                }
                1 => {
                    self.screen = Screen::PickApp {
                        use_back: true,
                        for_backup: true,
                    };
                    self.reset_list(4);
                }
                2 => {
                    self.screen = Screen::RestoreFilter;
                    self.reset_list(4);
                }
                _ => self.should_quit = true,
            },
            Screen::PickApp {
                use_back,
                for_backup,
            } => {
                if idx == 3 {
                    if *use_back {
                        self.screen = Screen::MainMenu;
                        self.reset_list(4);
                    } else {
                        self.should_quit = true;
                    }
                    return;
                }
                let editors = match idx {
                    0 => vec![Editor::Cursor],
                    1 => vec![Editor::VsCode],
                    _ => vec![Editor::Cursor, Editor::VsCode],
                };
                if *for_backup {
                    self.screen = Screen::BackupMode { editors };
                    self.reset_list(3);
                } else {
                    self.start_clean_editors(editors);
                }
            }
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
                if idx == 5 {
                    self.pending_editors.clear();
                    self.screen = Screen::PickApp {
                        use_back: false,
                        for_backup: false,
                    };
                    self.reset_list(4);
                    return;
                }
                if !self.ensure_editor(editor) {
                    return;
                }
                if idx == 3 {
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
                if idx == 4 {
                    self.finish_editor_clean();
                    return;
                }
                if let Some(sel) = self.plan_indices(idx, &stats) {
                    if sel.is_empty() {
                        self.finish_editor_clean();
                    } else {
                        self.execute_clean(editor, root, total, stats, sel);
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
                        self.reset_list(6);
                        return;
                    }
                }
                if item_idx >= stats.len() {
                    if selected.is_empty() {
                        self.status = StatusMsg::Dim(t(Msg::NothingSelected).to_string());
                        self.screen = Self::scan_screen(editor, root, total, stats);
                        self.reset_list(6);
                    } else if self.ensure_editor(editor) {
                        self.execute_clean(editor, root, total, stats, selected);
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
            Screen::BackupMode { editors } => {
                if idx == 2 {
                    self.screen = Screen::PickApp {
                        use_back: true,
                        for_backup: true,
                    };
                    self.reset_list(4);
                    return;
                }
                let include_state = idx == 1;
                self.screen = Screen::BackupName {
                    editors: editors.clone(),
                    include_state,
                };
                self.name_input = Input::default();
            }
            Screen::RestoreFilter => {
                if idx == 3 {
                    self.screen = Screen::MainMenu;
                    self.reset_list(4);
                    return;
                }
                let filter = match idx {
                    0 => Some(Editor::Cursor),
                    1 => Some(Editor::VsCode),
                    _ => None,
                };
                let backups = backup::list_backups(filter);
                if backups.is_empty() {
                    self.status = StatusMsg::Warn(t(Msg::NoBackups).to_string());
                    self.screen = Screen::RestoreFilter;
                    self.reset_list(4);
                } else {
                    self.screen = Screen::RestoreManage { filter, backups };
                    self.reset_list(3);
                }
            }
            Screen::RestoreManage { filter, backups } => {
                let filter = *filter;
                let backups = backups.clone();
                match idx {
                    0 => {
                        let n = backups.len() + 1;
                        self.screen = Screen::RestorePick { filter, backups };
                        self.reset_list(n);
                    }
                    1 => {
                        let n = backups.len() + 1;
                        self.checkbox = vec![false; n];
                        self.screen = Screen::DeleteMulti { filter, backups };
                        self.reset_list(n);
                    }
                    _ => {
                        self.screen = Screen::RestoreFilter;
                        self.reset_list(4);
                    }
                }
            }
            Screen::RestorePick { filter, backups } => {
                if idx >= backups.len() {
                    self.screen = Screen::RestoreManage {
                        filter: *filter,
                        backups: backups.clone(),
                    };
                    self.reset_list(3);
                    return;
                }
                let chosen = backups[idx].clone();
                let return_to = self.screen.clone();
                self.screen = Screen::Confirm {
                    prompt: format!("{}\n{}", t(Msg::ConfirmRestore), chosen.label()),
                    action: ConfirmAction::Restore(chosen),
                    return_to: Box::new(return_to),
                };
                self.reset_list(2);
            }
            Screen::DeleteMulti { filter, backups } => {
                if idx >= backups.len() {
                    self.screen = Screen::RestoreManage {
                        filter: *filter,
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
                if idx == 1 {
                    self.screen = *return_to.clone();
                    let len = self.current_labels().len().max(1);
                    self.reset_list(len);
                    return;
                }
                match action.clone() {
                    ConfirmAction::Restore(info) => {
                        let editor = match info.app.as_str() {
                            "vscode" => Editor::VsCode,
                            _ => Editor::Cursor,
                        };
                        if self.ensure_editor(editor) {
                            match backup::restore_backup(&info) {
                                Ok(()) => {
                                    self.did_work = true;
                                    self.status = StatusMsg::Ok(format!(
                                        "{} — {}",
                                        t(Msg::RestoreDone),
                                        t(Msg::PreRestoreBackup)
                                    ));
                                }
                                Err(e) => {
                                    self.status =
                                        StatusMsg::Warn(format!("{}: {e}", t(Msg::RestoreFail)));
                                }
                            }
                        }
                        self.screen = Screen::MainMenu;
                        self.reset_list(4);
                    }
                    ConfirmAction::DeleteBackups(infos) => {
                        for info in &infos {
                            match backup::delete_backup(info) {
                                Ok(()) => {
                                    self.did_work = true;
                                    self.status = StatusMsg::Ok(format!(
                                        "{} · {}",
                                        t(Msg::DeleteDone),
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
            Screen::BackupName { .. } => self.handle_name_key(key),
            Screen::DeleteMulti { filter, backups } => {
                let len = backups.len() + 1;
                match key.code {
                    KeyCode::Esc => {
                        self.screen = Screen::RestoreManage {
                            filter: *filter,
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
                        if selected.is_empty() || self.checkbox.get(back_idx).copied().unwrap_or(false)
                        {
                            self.screen = Screen::RestoreManage {
                                filter: *filter,
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
        let Screen::BackupName {
            editors,
            include_state,
        } = self.screen.clone()
        else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::BackupMode { editors };
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
                let mut any = false;
                for editor in &editors {
                    if !self.ensure_editor(*editor) {
                        continue;
                    }
                    match backup::create_backup(*editor, include_state, custom.as_deref()) {
                        Ok(info) => {
                            any = true;
                            self.status = StatusMsg::Ok(format!(
                                "{} — {} ({})",
                                t(Msg::BackupDone),
                                info.name,
                                fmt_size(info.bytes)
                            ));
                        }
                        Err(e) => {
                            self.status =
                                StatusMsg::Warn(format!("{}: {e}", t(Msg::BackupFail)));
                        }
                    }
                }
                if any {
                    self.did_work = true;
                }
                self.screen = Screen::MainMenu;
                self.reset_list(4);
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
            Screen::PickApp { .. } => Screen::MainMenu,
            Screen::Scanning { .. } => {
                self.pending_editors.clear();
                Screen::PickApp {
                    use_back: false,
                    for_backup: false,
                }
            }
            Screen::ScanAndPlan { .. } => {
                self.pending_editors.clear();
                Screen::PickApp {
                    use_back: false,
                    for_backup: false,
                }
            }
            Screen::CustomClean {
                editor,
                root,
                total,
                stats,
                ..
            } => Self::scan_screen(editor, root, total, stats),
            Screen::BackupMode { .. } => Screen::PickApp {
                use_back: true,
                for_backup: true,
            },
            Screen::BackupName { .. } => return,
            Screen::RestoreFilter => Screen::MainMenu,
            Screen::RestoreManage { .. } => Screen::RestoreFilter,
            Screen::RestorePick { filter, backups } => Screen::RestoreManage { filter, backups },
            Screen::DeleteMulti { filter, backups } => Screen::RestoreManage { filter, backups },
            Screen::Confirm { return_to, .. } => *return_to.clone(),
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
                    Screen::PickApp { .. } => t(Msg::PickApp),
                    Screen::BackupMode { .. } => t(Msg::BackupMode),
                    Screen::RestoreFilter => t(Msg::PickApp),
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
