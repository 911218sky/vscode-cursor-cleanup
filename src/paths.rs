use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Editor {
    Cursor,
    VsCode,
}

impl Editor {
    pub fn display_name(self) -> &'static str {
        match self {
            Editor::Cursor => "Cursor",
            Editor::VsCode => "VSCode",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Editor::Cursor => "cursor",
            Editor::VsCode => "vscode",
        }
    }

    /// Process names for Unix detect/kill. Windows uses `windows_images` instead.
    #[cfg(not(windows))]
    pub fn process_names(self) -> &'static [&'static str] {
        match self {
            Editor::Cursor => &["Cursor", "cursor"],
            Editor::VsCode => &["Code", "code"],
        }
    }

    pub fn data_dir(self) -> Option<PathBuf> {
        #[cfg(windows)]
        {
            let base = std::env::var_os("APPDATA").map(PathBuf::from)?;
            Some(match self {
                Editor::Cursor => base.join("Cursor"),
                Editor::VsCode => base.join("Code"),
            })
        }
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from)?;
            let support = home.join("Library").join("Application Support");
            Some(match self {
                Editor::Cursor => support.join("Cursor"),
                Editor::VsCode => support.join("Code"),
            })
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from)?;
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            Some(match self {
                Editor::Cursor => base.join("Cursor"),
                Editor::VsCode => base.join("Code"),
            })
        }
        #[cfg(not(any(windows, unix)))]
        {
            None
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
    State,
}

#[derive(Debug, Clone)]
pub struct CleanTarget {
    pub kind: TargetKind,
    pub risk: Risk,
    pub rel_paths: &'static [&'static str],
}

pub fn clean_targets() -> Vec<CleanTarget> {
    vec![
        CleanTarget {
            kind: TargetKind::CachedData,
            risk: Risk::Safe,
            rel_paths: &["CachedData"],
        },
        CleanTarget {
            kind: TargetKind::ExtVsix,
            risk: Risk::Safe,
            rel_paths: &["CachedExtensionVSIXs"],
        },
        CleanTarget {
            kind: TargetKind::Gpu,
            risk: Risk::Safe,
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
            rel_paths: &["logs"],
        },
        CleanTarget {
            kind: TargetKind::History,
            risk: Risk::Medium,
            rel_paths: &["User/History"],
        },
        CleanTarget {
            kind: TargetKind::State,
            risk: Risk::High,
            rel_paths: &["User/globalStorage/state.vscdb"],
        },
    ]
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
        use std::time::{SystemTime, UNIX_EPOCH};
        let dur = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = dur.as_secs();
        let ms = dur.subsec_millis();
        let (y, m, d, hh, mm, ss) = unix_to_ymd_hms(secs);
        format!("{y:04}-{m:02}-{d:02}-{hh:02}-{mm:02}-{ss:02}-{ms:03}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_targets_stay_under_user_data() {
        for t in clean_targets() {
            for rel in t.rel_paths {
                assert!(!rel.starts_with('/') && !rel.starts_with('\\'));
                assert!(!rel.contains(".."));
            }
        }
    }

    #[test]
    fn editor_slugs_stable() {
        assert_eq!(Editor::Cursor.slug(), "cursor");
        assert_eq!(Editor::VsCode.slug(), "vscode");
    }

    #[test]
    fn timestamp_has_millis() {
        let ts = timestamp();
        // YYYY-MM-DD-HH-MM-SS-mmm
        assert!(ts.len() >= 23, "timestamp too short: {ts}");
        assert_eq!(ts.matches('-').count(), 6);
    }
}

#[cfg(not(windows))]
fn unix_to_ymd_hms(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i64;
    let tod = (secs % 86400) as u32;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    // Civil from days since 1970-01-01 (Howard Hinnant)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, hh, mm, ss)
}
