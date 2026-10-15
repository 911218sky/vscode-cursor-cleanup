//! Full uninstall of Cursor IDE + Agent CLI (install + known leftovers).
//! Does **not** touch project-local `.cursor/` folders.

use crate::cleanup::force_quit_scope;
use crate::fsutil::{fmt_size, remove_best_effort};
use crate::i18n::{self, t, Msg};
use crate::paths::{DataScope, Editor};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Default)]
pub struct UninstallReport {
    pub uninstaller_ran: bool,
    pub uninstaller_ok: bool,
    pub removed: Vec<(PathBuf, u64)>,
    pub failed: Vec<PathBuf>,
    pub path_cleaned: bool,
    pub notes: Vec<String>,
}

impl UninstallReport {
    pub fn any_errors(&self) -> bool {
        !self.failed.is_empty() || (self.uninstaller_ran && !self.uninstaller_ok)
    }
}

/// Whether this process is running elevated (Administrator) on Windows.
pub fn is_process_elevated() -> bool {
    #[cfg(windows)]
    {
        Command::new("net")
            .arg("session")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn powershell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Relaunch this executable elevated with `--uninstall-run` (Windows UAC).
/// Returns true if the elevated process was started (and waited for).
pub fn relaunch_elevated_uninstall() -> bool {
    #[cfg(windows)]
    {
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let lang = i18n::lang().code();
        let file = powershell_quote(&exe.display().to_string());
        let arg_list = format!("'--uninstall-run','--lang','{}'", lang.replace('\'', "''"));
        let cmd = format!(
            "Start-Process -FilePath {file} -ArgumentList {arg_list} -Verb RunAs -Wait"
        );
        Command::new("powershell")
            .args(["-NoProfile", "-Command", &cmd])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Find Cursor's official Uninstall.exe under LocalAppData\\Programs.
pub fn find_windows_uninstaller() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        let programs = local.join("Programs");
        let mut dirs = Vec::new();
        for preferred in ["cursor", "Cursor"] {
            let p = programs.join(preferred);
            if p.is_dir() {
                dirs.push(p);
            }
        }
        if let Ok(entries) = std::fs::read_dir(&programs) {
            for ent in entries.flatten() {
                let name = ent.file_name();
                let n = name.to_string_lossy();
                if n.to_ascii_lowercase().starts_with("cursor") && ent.path().is_dir() {
                    let p = ent.path();
                    if !dirs.iter().any(|d| d == &p) {
                        dirs.push(p);
                    }
                }
            }
        }
        for dir in dirs {
            if let Some(u) = find_uninstaller_in_dir(&dir) {
                return Some(u);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn find_uninstaller_in_dir(dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return None;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        if !p.is_file() {
            continue;
        }
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".exe") && lower.contains("uninstall")) {
            continue;
        }
        let score = if lower == "uninstall.exe" {
            0
        } else if lower.starts_with("uninstall") {
            1
        } else {
            2
        };
        candidates.push((score, p));
    }
    candidates.sort_by_key(|(s, _)| *s);
    candidates.into_iter().map(|(_, p)| p).next()
}

fn push_if_exists(out: &mut Vec<PathBuf>, p: PathBuf) {
    if p.exists() {
        out.push(p);
    }
}

/// Known leftover / install paths that still exist on disk.
pub fn leftover_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();

    #[cfg(windows)]
    {
        if let Some(roaming) = std::env::var_os("APPDATA").map(PathBuf::from) {
            push_if_exists(&mut out, roaming.join("Cursor"));
            let sm = roaming
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            collect_start_menu_cursor(&sm, &mut out);
        }
        if let Some(home) = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
        {
            push_if_exists(&mut out, home.join(".cursor"));
            push_if_exists(&mut out, home.join(".cursor-tutor"));
            push_if_exists(&mut out, home.join(".cursor.json"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            push_if_exists(&mut out, local.join("cursor-updater"));
            push_if_exists(&mut out, local.join("cursor-agent"));
            if let Ok(entries) = std::fs::read_dir(&local) {
                for ent in entries.flatten() {
                    let name = ent.file_name();
                    let n = name.to_string_lossy();
                    if n.to_ascii_lowercase().starts_with("cursor") {
                        push_if_exists(&mut out, ent.path());
                    }
                }
            }
            let programs = local.join("Programs");
            if let Ok(entries) = std::fs::read_dir(&programs) {
                for ent in entries.flatten() {
                    let name = ent.file_name();
                    let n = name.to_string_lossy();
                    if n.to_ascii_lowercase().starts_with("cursor") {
                        push_if_exists(&mut out, ent.path());
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        push_if_exists(&mut out, PathBuf::from("/Applications/Cursor.app"));
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let support = home.join("Library").join("Application Support");
            push_if_exists(&mut out, support.join("Cursor"));
            push_if_exists(&mut out, support.join("Caches").join("cursor-updater"));
            let caches = home.join("Library").join("Caches");
            let td = "com.todesktop.230313mzl4w4u92";
            push_if_exists(&mut out, caches.join(td));
            push_if_exists(&mut out, caches.join(format!("{td}.ShipIt")));
            push_if_exists(
                &mut out,
                home.join("Library").join("HTTPStorages").join(td),
            );
            push_if_exists(&mut out, home.join("Library").join("Logs").join("Cursor"));
            push_if_exists(
                &mut out,
                home.join("Library")
                    .join("Preferences")
                    .join(format!("{td}.plist")),
            );
            let by_host = home.join("Library").join("Preferences").join("ByHost");
            if let Ok(entries) = std::fs::read_dir(&by_host) {
                for ent in entries.flatten() {
                    let name = ent.file_name();
                    let n = name.to_string_lossy();
                    if n.starts_with(&format!("{td}.ShipIt")) {
                        push_if_exists(&mut out, ent.path());
                    }
                }
            }
            push_if_exists(
                &mut out,
                home.join("Library")
                    .join("Saved Application State")
                    .join(format!("{td}.savedState")),
            );
            let recent = support
                .join("com.apple.sharedfilelist")
                .join("com.apple.LSSharedFileList.ApplicationRecentDocuments");
            if let Ok(entries) = std::fs::read_dir(&recent) {
                for ent in entries.flatten() {
                    let name = ent.file_name();
                    let n = name.to_string_lossy();
                    if n.contains("230313mzl4w4u92") {
                        push_if_exists(&mut out, ent.path());
                    }
                }
            }
            push_if_exists(&mut out, home.join(".cursor"));
            push_if_exists(&mut out, home.join(".cursor-tutor"));
            push_if_exists(&mut out, home.join(".cursor.json"));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            push_if_exists(&mut out, config.join("Cursor"));
            push_if_exists(&mut out, home.join(".cursor"));
            push_if_exists(&mut out, home.join(".cursor-tutor"));
            push_if_exists(&mut out, home.join(".cursor.json"));
        }
        for cand in [
            PathBuf::from("/usr/bin/cursor"),
            PathBuf::from("/usr/local/bin/cursor"),
            PathBuf::from("/opt/cursor"),
        ] {
            push_if_exists(&mut out, cand);
        }
    }

    let mut seen = Vec::new();
    out.retain(|p| {
        if seen.iter().any(|s: &PathBuf| s == p) {
            false
        } else {
            seen.push(p.clone());
            true
        }
    });
    out
}

#[cfg(windows)]
fn collect_start_menu_cursor(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.to_ascii_lowercase().contains("cursor") {
            push_if_exists(out, p);
        }
    }
}

#[cfg(windows)]
fn clean_cursor_agent_user_path() -> bool {
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return false;
    };
    let agent = local.join("cursor-agent");
    let q = powershell_quote(&agent.display().to_string());
    let script = format!(
        "$target = {q}.TrimEnd('\\').ToLowerInvariant(); \
         $p = [Environment]::GetEnvironmentVariable('PATH','User'); \
         if (-not $p) {{ exit 0 }}; \
         $parts = $p -split ';' | Where-Object {{ \
           $t = $_.Trim().TrimEnd('\\').ToLowerInvariant(); \
           $t -and ($t -ne $target) -and ($t -notlike ($target + '\\*')) \
         }}; \
         [Environment]::SetEnvironmentVariable('PATH', ($parts -join ';'), 'User')"
    );
    Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Full uninstall: force-quit → official uninstaller (Windows) → wipe leftovers → PATH.
pub fn run_full_uninstall(mut progress: impl FnMut(&str)) -> UninstallReport {
    let mut report = UninstallReport::default();

    progress(t(Msg::UninstallStepQuit));
    if !force_quit_scope(Editor::Cursor, DataScope::Both, true) {
        report.notes.push(t(Msg::ForceQuitFail).to_string());
    }

    #[cfg(windows)]
    {
        if let Some(uninst) = find_windows_uninstaller() {
            progress(&format!(
                "{} — {}",
                t(Msg::UninstallStepUninstaller),
                uninst.display()
            ));
            report.uninstaller_ran = true;
            match Command::new(&uninst).status() {
                Ok(st) => {
                    report.uninstaller_ok = st.success();
                    if !st.success() {
                        report.notes.push(format!(
                            "{} ({})",
                            t(Msg::UninstallUninstallerFail),
                            st.code()
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "?".into())
                        ));
                    }
                }
                Err(e) => {
                    report.uninstaller_ok = false;
                    report
                        .notes
                        .push(format!("{}: {e}", t(Msg::UninstallUninstallerFail)));
                }
            }
        } else {
            report
                .notes
                .push(t(Msg::UninstallNoUninstaller).to_string());
        }
    }

    progress(t(Msg::UninstallStepLeftovers));
    for path in leftover_paths() {
        let display = path.display().to_string();
        let (freed, _errors) = remove_best_effort(&path);
        if path.exists() {
            report.failed.push(path);
            progress(&format!("  ✖ {display}"));
        } else {
            report.removed.push((path, freed));
            progress(&format!("  ✔ {display} ({})", fmt_size(freed)));
        }
    }

    #[cfg(windows)]
    {
        progress(t(Msg::UninstallStepPath));
        report.path_cleaned = clean_cursor_agent_user_path();
    }

    report
}

/// CLI / elevated entry: print progress to stdout. Returns true when fully successful.
pub fn run_uninstall_cli() -> bool {
    println!();
    println!("  {}", t(Msg::UninstallRunning));
    println!();
    let report = run_full_uninstall(|msg| {
        println!("  {msg}");
        let _ = io::stdout().flush();
    });
    println!();
    for note in &report.notes {
        println!("  ⚠ {note}");
    }
    if report.uninstaller_ran {
        if report.uninstaller_ok {
            println!("  ✔ {}", t(Msg::UninstallUninstallerOk));
        } else {
            println!("  ⚠ {}", t(Msg::UninstallUninstallerFail));
        }
    }
    println!(
        "  {} {} · {} {}",
        t(Msg::UninstallRemovedCount),
        report.removed.len(),
        t(Msg::UninstallFailedCount),
        report.failed.len()
    );
    for p in &report.failed {
        println!("  ⚠ {}", p.display());
    }
    if report.path_cleaned {
        println!("  ✔ {}", t(Msg::UninstallPathCleaned));
    }
    if report.any_errors() {
        println!("  ⚠ {}", t(Msg::UninstallDonePartial));
        false
    } else {
        println!("  ✔ {}", t(Msg::UninstallDone));
        true
    }
}
