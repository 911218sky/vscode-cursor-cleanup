use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Editor {
    Cursor,
}

/// Which on-disk tree a cleanup target lives under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataRoot {
    /// Cursor IDE app data (`%APPDATA%/Cursor`, …).
    Ide,
    /// Cursor / Agent CLI home (`~/.cursor`).
    CursorHome,
}

/// User-facing scope: IDE data, Agent CLI data, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataScope {
    Ide,
    Cli,
    Both,
}

impl DataScope {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ide" | "editor" | "app" => Some(Self::Ide),
            "cli" | "agent" | "agent-cli" => Some(Self::Cli),
            "both" | "all" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn includes_ide(self) -> bool {
        matches!(self, Self::Ide | Self::Both)
    }

    pub fn includes_cli(self) -> bool {
        matches!(self, Self::Cli | Self::Both)
    }

    /// Filter by logical product, not only `DataRoot`
    /// (`auth.json` lives under the IDE data dir but belongs to CLI).
    pub fn includes_kind(self, kind: TargetKind) -> bool {
        match kind {
            TargetKind::CachedData
            | TargetKind::ExtVsix
            | TargetKind::Gpu
            | TargetKind::Logs
            | TargetKind::History
            | TargetKind::State => self.includes_ide(),
            TargetKind::AgentCliCache
            | TargetKind::AgentCliState
            | TargetKind::AgentCliAuth => self.includes_cli(),
        }
    }
}

impl Editor {
    pub fn display_name(self) -> &'static str {
        "Cursor"
    }

    pub fn slug(self) -> &'static str {
        "cursor"
    }

    /// Process names for Unix detect/kill. Windows uses `windows_images` instead.
    /// Includes Agent CLI (`agent` / `cursor-agent`).
    #[cfg(not(windows))]
    pub fn process_names(self) -> &'static [&'static str] {
        let _ = self;
        &["Cursor", "cursor", "agent", "cursor-agent"]
    }

    pub fn data_dir(self) -> Option<PathBuf> {
        self.data_root(DataRoot::Ide)
    }

    /// `~/.cursor` — Agent CLI state / caches (shared with newer Cursor layouts).
    pub fn cursor_home(self) -> Option<PathBuf> {
        self.data_root(DataRoot::CursorHome)
    }

    pub fn data_root(self, root: DataRoot) -> Option<PathBuf> {
        let _ = self;
        match root {
            DataRoot::Ide => {
                #[cfg(windows)]
                {
                    let base = std::env::var_os("APPDATA").map(PathBuf::from)?;
                    Some(base.join("Cursor"))
                }
                #[cfg(target_os = "macos")]
                {
                    let home = std::env::var_os("HOME").map(PathBuf::from)?;
                    let support = home.join("Library").join("Application Support");
                    Some(support.join("Cursor"))
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    let home = std::env::var_os("HOME").map(PathBuf::from)?;
                    let base = std::env::var_os("XDG_CONFIG_HOME")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| home.join(".config"));
                    Some(base.join("Cursor"))
                }
                #[cfg(not(any(windows, unix)))]
                {
                    None
                }
            }
            DataRoot::CursorHome => {
                #[cfg(windows)]
                {
                    let home = std::env::var_os("USERPROFILE")
                        .or_else(|| std::env::var_os("HOME"))
                        .map(PathBuf::from)?;
                    Some(home.join(".cursor"))
                }
                #[cfg(not(windows))]
                {
                    let home = std::env::var_os("HOME").map(PathBuf::from)?;
                    Some(home.join(".cursor"))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Risk {
    Safe,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy)]
pub enum TargetKind {
    CachedData,
    ExtVsix,
    Gpu,
    Logs,
    History,
    /// Agent CLI caches under `~/.cursor` (not the CLI install itself).
    AgentCliCache,
    /// Agent CLI local state / chat folders under `~/.cursor`.
    AgentCliState,
    State,
    /// Agent CLI login file (`auth.json`) next to IDE data.
    AgentCliAuth,
}

#[derive(Debug, Clone)]
pub struct CleanTarget {
    pub kind: TargetKind,
    pub risk: Risk,
    pub root: DataRoot,
    pub rel_paths: &'static [&'static str],
}

#[allow(dead_code)]
pub fn clean_targets() -> Vec<CleanTarget> {
    clean_targets_for(DataScope::Both)
}

pub fn clean_targets_for(scope: DataScope) -> Vec<CleanTarget> {
    let all = [
        CleanTarget {
            kind: TargetKind::CachedData,
            risk: Risk::Safe,
            root: DataRoot::Ide,
            rel_paths: &["CachedData"],
        },
        CleanTarget {
            kind: TargetKind::ExtVsix,
            risk: Risk::Safe,
            root: DataRoot::Ide,
            rel_paths: &["CachedExtensionVSIXs"],
        },
        CleanTarget {
            kind: TargetKind::Gpu,
            risk: Risk::Safe,
            root: DataRoot::Ide,
            rel_paths: &[
                "GPUCache",
                "Cache",
                "Code Cache",
                "DawnCache",
                "DawnGraphiteCache",
                "DawnWebGPUCache",
            ],
        },
        CleanTarget {
            kind: TargetKind::Logs,
            risk: Risk::Safe,
            root: DataRoot::Ide,
            rel_paths: &["logs"],
        },
        CleanTarget {
            kind: TargetKind::AgentCliCache,
            risk: Risk::Safe,
            root: DataRoot::CursorHome,
            rel_paths: &["statsig-cache.json", "ai-tracking"],
        },
        CleanTarget {
            kind: TargetKind::History,
            risk: Risk::Medium,
            root: DataRoot::Ide,
            rel_paths: &["User/History"],
        },
        CleanTarget {
            kind: TargetKind::AgentCliState,
            risk: Risk::Medium,
            root: DataRoot::CursorHome,
            // Keep extensions / plugins / skills / cli-config — only CLI runtime state.
            rel_paths: &["agent-cli-state.json", "chats", "projects"],
        },
        CleanTarget {
            kind: TargetKind::State,
            risk: Risk::High,
            root: DataRoot::Ide,
            rel_paths: &[
                "User/globalStorage/state.vscdb",
                "User/globalStorage/state.vscdb-wal",
                "User/globalStorage/state.vscdb-shm",
            ],
        },
        CleanTarget {
            kind: TargetKind::AgentCliAuth,
            risk: Risk::High,
            root: DataRoot::Ide,
            rel_paths: &["auth.json", "auth.json.bak"],
        },
    ];
    all.into_iter()
        .filter(|t| scope.includes_kind(t.kind))
        .collect()
}

/// Relative paths under IDE data dir for Agent CLI login (backup / restore).
pub fn agent_cli_auth_rel_paths() -> &'static [&'static str] {
    &["auth.json", "auth.json.bak"]
}

/// Plan menu choice → target indices by risk.
/// `0` conservative (Safe), `1` standard (Safe|Medium), `2` deep (all), else `None`.
pub fn plan_target_indices(choice: usize, risks: &[Risk]) -> Option<Vec<usize>> {
    match choice {
        0 => Some(
            risks
                .iter()
                .enumerate()
                .filter(|(_, r)| matches!(r, Risk::Safe))
                .map(|(i, _)| i)
                .collect(),
        ),
        1 => Some(
            risks
                .iter()
                .enumerate()
                .filter(|(_, r)| matches!(r, Risk::Safe | Risk::Medium))
                .map(|(i, _)| i)
                .collect(),
        ),
        2 => Some((0..risks.len()).collect()),
        _ => None,
    }
}

/// True when `rel` is a safe relative path under a data root (no `..`, not absolute).
pub fn is_safe_rel_path(rel: &str) -> bool {
    if rel.is_empty() || rel.starts_with('/') || rel.starts_with('\\') {
        return false;
    }
    if PathBuf::from(rel).is_absolute() {
        return false;
    }
    !rel.split(['/', '\\']).any(|p| p == "..")
}

pub fn config_rel_paths() -> &'static [&'static str] {
    &[
        "User/settings.json",
        "User/keybindings.json",
        "User/snippets",
    ]
}

/// Directory containing this executable (portable install root).
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

/// Human-readable local stamp: `2026-10-06-11-42-05-123` (includes milliseconds)
pub fn timestamp() -> String {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        extern "system" {
            fn GetLocalTime(lp: *mut SystemTime);
        }
        let mut st = SystemTime {
            year: 0,
            month: 0,
            day_of_week: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            milliseconds: 0,
        };
        unsafe { GetLocalTime(&mut st) };
        format!(
            "{:04}-{:02}-{:02}-{:02}-{:02}-{:02}-{:03}",
            st.year, st.month, st.day, st.hour, st.minute, st.second, st.milliseconds
        )
    }
    #[cfg(not(windows))]
    {
        use chrono::{Datelike, Timelike};
        let now = chrono::Local::now();
        format!(
            "{:04}-{:02}-{:02}-{:02}-{:02}-{:02}-{:03}",
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
            now.timestamp_subsec_millis()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_targets_stay_under_user_data() {
        for t in clean_targets() {
            for rel in t.rel_paths {
                assert!(is_safe_rel_path(rel), "unsafe rel: {rel}");
            }
        }
    }

    #[test]
    fn plan_standard_excludes_high() {
        let risks: Vec<Risk> = clean_targets().iter().map(|t| t.risk).collect();
        let std = plan_target_indices(1, &risks).unwrap();
        assert!(!std.is_empty());
        for i in &std {
            assert!(
                !matches!(risks[*i], Risk::High),
                "standard must not include High index {i}"
            );
        }
        let cons = plan_target_indices(0, &risks).unwrap();
        for i in &cons {
            assert!(matches!(risks[*i], Risk::Safe));
        }
        let deep = plan_target_indices(2, &risks).unwrap();
        assert!(deep.iter().any(|&i| matches!(risks[i], Risk::High)));
        assert!(plan_target_indices(3, &risks).is_none());
    }

    #[test]
    fn scope_filters_kinds() {
        let ide = clean_targets_for(DataScope::Ide);
        assert!(ide.iter().all(|t| DataScope::Ide.includes_kind(t.kind)));
        assert!(ide
            .iter()
            .any(|t| matches!(t.kind, TargetKind::State)));
        assert!(!ide
            .iter()
            .any(|t| matches!(t.kind, TargetKind::AgentCliAuth)));

        let cli = clean_targets_for(DataScope::Cli);
        assert!(cli.iter().all(|t| DataScope::Cli.includes_kind(t.kind)));
        assert!(cli
            .iter()
            .any(|t| matches!(t.kind, TargetKind::AgentCliAuth)));
        assert!(!cli
            .iter()
            .any(|t| matches!(t.kind, TargetKind::State)));

        assert_eq!(
            clean_targets_for(DataScope::Both).len(),
            clean_targets().len()
        );
    }

    #[test]
    fn scope_parse() {
        assert_eq!(DataScope::parse("ide"), Some(DataScope::Ide));
        assert_eq!(DataScope::parse("CLI"), Some(DataScope::Cli));
        assert_eq!(DataScope::parse("both"), Some(DataScope::Both));
        assert_eq!(DataScope::parse("nope"), None);
    }

    #[test]
    fn is_safe_rel_path_rejects_traversal() {
        assert!(is_safe_rel_path("User/History"));
        assert!(!is_safe_rel_path("../etc/passwd"));
        assert!(!is_safe_rel_path("User/../../outside"));
        assert!(!is_safe_rel_path("/abs"));
    }

    #[test]
    fn editor_slugs_stable() {
        assert_eq!(Editor::Cursor.slug(), "cursor");
    }

    #[test]
    fn timestamp_has_millis() {
        let ts = timestamp();
        // YYYY-MM-DD-HH-MM-SS-mmm
        assert!(ts.len() >= 23, "timestamp too short: {ts}");
        assert_eq!(ts.matches('-').count(), 6);
    }
}
