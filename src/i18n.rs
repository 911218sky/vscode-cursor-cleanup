use std::sync::Mutex;

static LANG: Mutex<Lang> = Mutex::new(Lang::ZhTw);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// 繁體中文（預設）
    ZhTw,
    /// 简体中文
    ZhCn,
    /// English
    En,
}

impl Lang {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "zh-tw" | "zh_tw" | "zh-hant" | "tw" | "trad" => Some(Lang::ZhTw),
            "zh-cn" | "zh_cn" | "zh-hans" | "cn" | "simp" => Some(Lang::ZhCn),
            "en" | "en-us" | "en_us" | "english" => Some(Lang::En),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn code(self) -> &'static str {
        match self {
            Lang::ZhTw => "zh-TW",
            Lang::ZhCn => "zh-CN",
            Lang::En => "en",
        }
    }
}

pub fn set_lang(lang: Lang) {
    if let Ok(mut g) = LANG.lock() {
        *g = lang;
    }
}

pub fn lang() -> Lang {
    LANG.lock().map(|g| *g).unwrap_or(Lang::ZhTw)
}

macro_rules! tr {
    ($($variant:ident => { tw: $tw:expr, cn: $cn:expr, en: $en:expr }),+ $(,)?) => {
        #[derive(Clone, Copy)]
        pub enum Msg {
            $($variant),+
        }

        pub fn t(msg: Msg) -> &'static str {
            match (lang(), msg) {
                $(
                    (Lang::ZhTw, Msg::$variant) => $tw,
                    (Lang::ZhCn, Msg::$variant) => $cn,
                    (Lang::En, Msg::$variant) => $en,
                )+
            }
        }
    };
}

tr! {
    Back => { tw: "← 返回", cn: "← 返回", en: "← Back" },
    BannerHint => { tw: "滑鼠點選 / ↑↓ Enter  ·  Esc 返回", cn: "鼠标点选 / ↑↓ Enter  ·  Esc 返回", en: "Click / ↑↓ Enter  ·  Esc Back" },
    PauseExit => { tw: "按 Enter 關閉視窗…", cn: "按 Enter 关闭窗口…", en: "Press Enter to close…" },
    PressEnterMenu => { tw: "按 Enter 回主選單…", cn: "按 Enter 回主菜单…", en: "Press Enter for main menu…" },
    PickLang => { tw: "選擇語言 / Language", cn: "选择语言 / Language", en: "Language" },
    LangTw => { tw: "繁體中文", cn: "繁体中文", en: "Traditional Chinese" },
    LangCn => { tw: "简体中文", cn: "简体中文", en: "Simplified Chinese" },
    LangEn => { tw: "English", cn: "English", en: "English" },
    Exit => { tw: "退出", cn: "退出", en: "Exit" },
    TotalSize => { tw: "總占用", cn: "总占用", en: "Total" },
    Cleanable => { tw: "可清理", cn: "可清理", en: "Cleanable" },
    RiskSafe => { tw: "[安全]", cn: "[安全]", en: "[safe]" },
    RiskMedium => { tw: "[中等]", cn: "[中等]", en: "[medium]" },
    RiskHigh => { tw: "[高危]", cn: "[高危]", en: "[high]" },
    RiskSafeWord => { tw: "安全", cn: "安全", en: "safe" },
    RiskMediumWord => { tw: "中等", cn: "中等", en: "medium" },
    RiskHighWord => { tw: "高危", cn: "高危", en: "high" },
    PickPlan => { tw: "選擇清理方案", cn: "选择清理方案", en: "Choose a cleanup plan" },
    PlanCustom => { tw: "自訂 — 逐項確認", cn: "自定义 — 逐项确认", en: "Custom — confirm each item" },
    Clean => { tw: "清理", cn: "清理", en: "Clean" },
    Skip => { tw: "跳過", cn: "跳过", en: "Skip" },
    NothingSelected => { tw: "未選擇清理項", cn: "未选择清理项", en: "Nothing selected" },
    AboutToClean => { tw: "即將清理：", cn: "即将清理：", en: "Will clean:" },
    ForceQuitOk => { tw: "已強制關閉", cn: "已强制关闭", en: "Force-quit done" },
    ForceQuitFail => { tw: "無法完全關閉，檔案可能仍被鎖定", cn: "无法完全关闭，文件可能仍被锁定", en: "Could not fully quit; files may stay locked" },
    ForceQuitting => { tw: "正在強制關閉 Cursor…", cn: "正在强制关闭 Cursor…", en: "Force-quitting Cursor…" },
    ConfirmForceQuit => { tw: "正在執行，確定要強制關閉？未儲存的變更可能遺失。", cn: "正在运行，确定要强制关闭？未保存的更改可能丢失。", en: "is running — force quit? Unsaved changes may be lost." },
    EditorRunningAbort => { tw: "應用仍在執行，已略過清理（請先關閉，或改用 TUI 強制關閉）", cn: "应用仍在运行，已跳过清理（请先关闭，或改用 TUI 强制关闭）", en: "App still running — skipped cleanup (close it first, or use TUI force-quit)" },
    Done => { tw: "完成", cn: "完成", en: "done" },
    Partial => { tw: "部分", cn: "部分", en: "partial" },
    AllDone => { tw: "全部完成", cn: "全部完成", en: "All done" },
    TargetCachedData => { tw: "應用快取 CachedData", cn: "应用缓存 CachedData", en: "App cache CachedData" },
    TargetExt => { tw: "擴充安裝包快取", cn: "扩展安装包缓存", en: "Extension VSIX cache" },
    TargetGpu => { tw: "GPU / 渲染快取", cn: "GPU / 渲染缓存", en: "GPU / render caches" },
    TargetLogs => { tw: "日誌 logs", cn: "日志 logs", en: "logs" },
    TargetHistory => { tw: "本機編輯歷史 History", cn: "本地编辑历史 History", en: "Local edit history" },
    TargetState => { tw: "狀態庫 state.vscdb", cn: "状态库 state.vscdb", en: "State DB state.vscdb" },
    MainMenu => { tw: "主選單", cn: "主菜单", en: "Main menu" },
    MenuClean => { tw: "清理快取 / 狀態", cn: "清理缓存 / 状态", en: "Clean caches / state" },
    MenuBackup => { tw: "備份", cn: "备份", en: "Backup" },
    MenuRestore => { tw: "恢復 / 刪除備份", cn: "恢复 / 删除备份", en: "Restore / delete backups" },
    BackupMode => { tw: "備份內容", cn: "备份内容", en: "Backup contents" },
    BackupConfigOnly => { tw: "僅設定（settings / keybindings / snippets）", cn: "仅配置（settings / keybindings / snippets）", en: "Config only (settings / keybindings / snippets)" },
    BackupConfigState => { tw: "設定 + state.vscdb（推薦完整）", cn: "配置 + state.vscdb（推荐完整）", en: "Config + state.vscdb (recommended)" },
    BackupNameAsk => { tw: "備份名稱（資料夾名）", cn: "备份名称（文件夹名）", en: "Backup name (folder)" },
    BackupNameHint => { tw: "可自訂名稱；直接 Enter＝預設時間戳（如 cursor-backup-2026-10-07-…）", cn: "可自定义名称；直接 Enter＝默认时间戳（如 cursor-backup-2026-10-07-…）", en: "Custom name OK; Enter alone = default timestamp (e.g. cursor-backup-2026-10-07-…)" },
    BackupNameInvalid => { tw: "名稱無效，將改用預設時間戳", cn: "名称无效，将改用默认时间戳", en: "Invalid name — using default timestamp instead" },
    PreRestoreBackup => { tw: "恢復前已自動備份目前設定", cn: "恢复前已自动备份目前配置", en: "Auto-backed up current config before restore" },
    ConfirmDelete => { tw: "確定刪除所選備份？", cn: "确定删除所选备份？", en: "Delete selected backups?" },
    ConfirmRestore => { tw: "確定恢復此備份？", cn: "确定恢复此备份？", en: "Restore this backup?" },
    ConfirmRestoreRunning => { tw: "Cursor 正在執行 — 恢復前必須先關閉（下一步會詢問是否強制關閉）。", cn: "Cursor 正在运行 — 恢复前必须先关闭（下一步会询问是否强制关闭）。", en: "Cursor is running — must close before restore (you will be asked to force quit next)." },
    RestoreEditorRunning => { tw: "Cursor 仍在執行，無法寫入設定檔", cn: "Cursor 仍在运行，无法写入配置文件", en: "Cursor still running — cannot write config files" },
    ConfirmYes => { tw: "確定", cn: "确定", en: "Confirm" },
    ConfirmNo => { tw: "取消", cn: "取消", en: "Cancel" },
    MouseHint => { tw: "滑鼠點選 / ↑↓ Enter", cn: "鼠标点选 / ↑↓ Enter", en: "Click / ↑↓ Enter" },
    BackupDone => { tw: "備份完成", cn: "备份完成", en: "Backup done" },
    BackupFail => { tw: "備份失敗", cn: "备份失败", en: "Backup failed" },
    NoBackups => { tw: "找不到備份（程式旁的 backups 資料夾）", cn: "找不到备份（程序旁的 backups 文件夹）", en: "No backups found (backups/ next to the exe)" },
    BackupManage => { tw: "要做什麼？", cn: "要做什么？", en: "What do you want to do?" },
    ActionRestore => { tw: "恢復一份備份", cn: "恢复一份备份", en: "Restore one backup" },
    ActionDelete => { tw: "刪除備份（可多選）", cn: "删除备份（可多选）", en: "Delete backups (multi-select)" },
    PickBackup => { tw: "選擇要恢復的備份", cn: "选择要恢复的备份", en: "Select backup to restore" },
    PickDeleteBackups => { tw: "勾選要刪除的備份", cn: "勾选要删除的备份", en: "Select backups to delete" },
    DeleteMultiHint => { tw: "空白鍵勾選要刪的項目後 Enter；直接 Enter／Esc＝返回（不用勾選「返回」）", cn: "空格键勾选要删的项目后 Enter；直接 Enter／Esc＝返回（不用勾选「返回」）", en: "Space-check items then Enter to delete; bare Enter / Esc = back" },
    DeleteDone => { tw: "已刪除備份", cn: "已删除备份", en: "Backup deleted" },
    DeleteFail => { tw: "刪除失敗", cn: "删除失败", en: "Delete failed" },
    RestoreDone => { tw: "恢復完成 — 請重啟 Cursor", cn: "恢复完成 — 请重启 Cursor", en: "Restore done — please restart Cursor" },
    RestoreFail => { tw: "恢復失敗", cn: "恢复失败", en: "Restore failed" },
}

pub fn risk_tag(risk: crate::paths::Risk) -> &'static str {
    match risk {
        crate::paths::Risk::Safe => t(Msg::RiskSafe),
        crate::paths::Risk::Medium => t(Msg::RiskMedium),
        crate::paths::Risk::High => t(Msg::RiskHigh),
    }
}

pub fn risk_word(risk: crate::paths::Risk) -> &'static str {
    match risk {
        crate::paths::Risk::Safe => t(Msg::RiskSafeWord),
        crate::paths::Risk::Medium => t(Msg::RiskMediumWord),
        crate::paths::Risk::High => t(Msg::RiskHighWord),
    }
}

pub fn target_title(kind: crate::paths::TargetKind) -> &'static str {
    match kind {
        crate::paths::TargetKind::CachedData => t(Msg::TargetCachedData),
        crate::paths::TargetKind::ExtVsix => t(Msg::TargetExt),
        crate::paths::TargetKind::Gpu => t(Msg::TargetGpu),
        crate::paths::TargetKind::Logs => t(Msg::TargetLogs),
        crate::paths::TargetKind::History => t(Msg::TargetHistory),
        crate::paths::TargetKind::State => t(Msg::TargetState),
    }
}

pub fn plan_conservative(size: &str) -> String {
    match lang() {
        Lang::ZhTw => format!("保守清理 — 僅安全項（約 {size}）"),
        Lang::ZhCn => format!("保守清理 — 仅安全项（约 {size}）"),
        Lang::En => format!("Conservative — safe only (~{size})"),
    }
}

pub fn plan_standard(size: &str) -> String {
    match lang() {
        Lang::ZhTw => format!("標準清理 — 安全+編輯歷史（約 {size}）"),
        Lang::ZhCn => format!("标准清理 — 安全+编辑历史（约 {size}）"),
        Lang::En => format!("Standard — safe + history (~{size})"),
    }
}

pub fn plan_deep(size: &str) -> String {
    match lang() {
        Lang::ZhTw => format!("深度清理 — 含 state.vscdb（約 {size}）"),
        Lang::ZhCn => format!("深度清理 — 含 state.vscdb（约 {size}）"),
        Lang::En => format!("Deep — includes state.vscdb (~{size})"),
    }
}

pub fn fmt_running(name: &str) -> String {
    match lang() {
        Lang::ZhTw => format!("{name} 正在執行 — 部分檔案可能被鎖定"),
        Lang::ZhCn => format!("{name} 正在运行 — 部分文件可能被锁定"),
        Lang::En => format!("{name} is running — some files may be locked"),
    }
}

pub fn fmt_no_dir(name: &str, path: &str) -> String {
    match lang() {
        Lang::ZhTw => format!("{name} 目錄不存在: {path}"),
        Lang::ZhCn => format!("{name} 目录不存在: {path}"),
        Lang::En => format!("{name} folder missing: {path}"),
    }
}

pub fn fmt_no_resolve(name: &str) -> String {
    match lang() {
        Lang::ZhTw => format!("{name} 無法解析資料目錄"),
        Lang::ZhCn => format!("{name} 无法解析数据目录"),
        Lang::En => format!("{name}: cannot resolve data folder"),
    }
}

pub fn fmt_partial(size: &str, n: usize) -> String {
    match lang() {
        Lang::ZhTw => format!("{} {}（{} 個檔案被鎖）", t(Msg::Partial), size, n),
        Lang::ZhCn => format!("{} {}（{} 个文件被锁）", t(Msg::Partial), size, n),
        Lang::En => format!("{} {} ({} file(s) locked)", t(Msg::Partial), size, n),
    }
}

pub fn fmt_summary(name: &str, before: &str, after: &str, freed: &str) -> String {
    match lang() {
        Lang::ZhTw => format!("{name}  {before} → {after}  （應用內釋放約 {freed}）"),
        Lang::ZhCn => format!("{name}  {before} → {after}  （应用内释放约 {freed}）"),
        Lang::En => format!("{name}  {before} → {after}  (app folder freed ~{freed})"),
    }
}

pub fn help_text() -> String {
    match lang() {
        Lang::ZhTw => "\
cursor-cleanup  —  Cursor 清理 / 備份工具

用法:
  cursor-cleanup.exe
  cursor-cleanup.exe --scan
  cursor-cleanup.exe --lang en

選項:
  -l, --lang <zh-TW|zh-CN|en>     語言（預設 zh-TW）
  -y, --yes                       非互動，只清安全項（Cursor 執行中會略過）
      --scan                      只掃描占用
      --no-pause                  結束不暫停
  -h, --help                      說明
  -V, --version                   版本

無參數：TUI 互動（滑鼠 + 鍵盤）— 清理 / 備份 / 恢復／刪除備份
--scan / --yes：CLI 純文字輸出
備份位置：與程式同目錄的 backups\\\\
"
        .into(),
        Lang::ZhCn => "\
cursor-cleanup  —  Cursor 清理 / 备份工具

用法:
  cursor-cleanup.exe
  cursor-cleanup.exe --scan
  cursor-cleanup.exe --lang en

选项:
  -l, --lang <zh-TW|zh-CN|en>     语言（默认 zh-TW）
  -y, --yes                       非互动，只清安全项（Cursor 运行中会跳过）
      --scan                      只扫描占用
      --no-pause                  结束不暂停
  -h, --help                      帮助
  -V, --version                   版本

无参数：TUI 互动（鼠标 + 键盘）— 清理 / 备份 / 恢复／删除备份
--scan / --yes：CLI 纯文字输出
备份位置：与程序同目录的 backups\\\\
"
        .into(),
        Lang::En => "\
cursor-cleanup  —  Cursor clean / backup tool

Usage:
  cursor-cleanup.exe
  cursor-cleanup.exe --scan
  cursor-cleanup.exe --lang zh-TW

Options:
  -l, --lang <zh-TW|zh-CN|en>     Language (default zh-TW)
  -y, --yes                       Non-interactive, safe items only (skips if Cursor running)
      --scan                      Scan sizes only
      --no-pause                  Do not pause on exit
  -h, --help                      Help
  -V, --version                   Version

No args: TUI (mouse + keyboard) — Clean / Backup / Restore-or-delete
--scan / --yes: CLI stdout mode
Backups: backups\\\\ next to the executable
"
        .into(),
    }
}
