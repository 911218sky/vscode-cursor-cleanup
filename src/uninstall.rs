//! Full uninstall of Cursor IDE and/or Agent CLI (install + known leftovers).
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

pub use crate::elevate::is_process_elevated;

/// IDE uninstall (official Uninstall.exe / Program Files) needs elevation on Windows.
pub fn needs_elevation(scope: DataScope) -> bool {
    #[cfg(windows)]
    {
        scope.includes_ide() && !is_process_elevated()
    }
    #[cfg(not(windows))]
    {
        let _ = scope;
        false
    }
}

/// Relaunch this executable elevated with `--uninstall-run` (Windows UAC).
/// Returns true if the elevated process was started (and waited for).
pub fn relaunch_elevated_uninstall(scope: DataScope) -> bool {
    let scope_arg = match scope {
        DataScope::Ide => "ide",
        DataScope::Cli => "cli",
        DataScope::Both => "both",
    };
    crate::elevate::relaunch_elevated(&["--uninstall-run", "--scope", scope_arg])
}

/// Official uninstaller command (exe + args). Prefer silent Inno Setup when available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallerCmd {
    pub exe: PathBuf,
    pub args: Vec<String>,
    pub source: &'static str,
}

impl UninstallerCmd {
    pub fn display_line(&self) -> String {
        if self.args.is_empty() {
            format!("{} ({})", self.exe.display(), self.source)
        } else {
            format!(
                "{} {} ({})",
                self.exe.display(),
                self.args.join(" "),
                self.source
            )
        }
    }
}

/// Resolve Cursor's Windows uninstaller (registry → disk).
/// Supports system installs (`Program Files\\cursor\\unins000.exe`) and user installs.
pub fn find_windows_uninstaller() -> Option<PathBuf> {
    find_windows_uninstaller_cmd().map(|c| c.exe)
}

pub fn find_windows_uninstaller_cmd() -> Option<UninstallerCmd> {
    #[cfg(windows)]
    {
        if let Some(cmd) = find_uninstaller_from_registry() {
            return Some(cmd);
        }
        for dir in windows_install_dirs() {
            if let Some(exe) = find_uninstaller_in_dir(&dir) {
                // Inno Setup (`unins000.exe`): use /SILENT so the flow does not hang on a GUI.
                let args = if is_inno_unins(&exe) {
                    vec!["/SILENT".into()]
                } else {
                    Vec::new()
                };
                return Some(UninstallerCmd {
                    exe,
                    args,
                    source: "disk",
                });
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
fn is_inno_unins(exe: &Path) -> bool {
    exe.file_name()
        .and_then(|n| n.to_str())
        .map(|n| {
            let lower = n.to_ascii_lowercase();
            lower.starts_with("unins") && lower.ends_with(".exe") && !lower.contains("uninstall")
        })
        .unwrap_or(false)
}

#[cfg(windows)]
fn windows_install_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for root in [
        std::env::var_os("ProgramFiles").map(PathBuf::from),
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
        Some(PathBuf::from(r"C:\Program Files")),
        Some(PathBuf::from(r"C:\Program Files (x86)")),
    ]
    .into_iter()
    .flatten()
    {
        for name in ["cursor", "Cursor"] {
            let p = root.join(name);
            if p.is_dir() && !dirs.iter().any(|d| paths_equal(d.as_path(), p.as_path())) {
                dirs.push(p);
            }
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        let programs = local.join("Programs");
        for preferred in ["cursor", "Cursor"] {
            let p = programs.join(preferred);
            if p.is_dir() && !dirs.iter().any(|d| paths_equal(d.as_path(), p.as_path())) {
                dirs.push(p);
            }
        }
        if let Ok(entries) = std::fs::read_dir(&programs) {
            for ent in entries.flatten() {
                let name = ent.file_name();
                let n = name.to_string_lossy();
                if n.to_ascii_lowercase().starts_with("cursor") && ent.path().is_dir() {
                    let p = ent.path();
                    if !dirs.iter().any(|d| paths_equal(d.as_path(), p.as_path())) {
                        dirs.push(p);
                    }
                }
            }
        }
    }
    dirs
}

#[cfg(windows)]
fn paths_equal(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

#[cfg(not(windows))]
fn paths_equal(a: &Path, b: &Path) -> bool {
    a == b
}

/// Parse `"C:\\Path\\unins000.exe" /SILENT` style uninstall strings.
pub fn parse_uninstall_command_line(raw: &str) -> Option<(PathBuf, Vec<String>)> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let (exe, rest): (PathBuf, String) = if let Some(stripped) = s.strip_prefix('"') {
        let end = stripped.find('"')?;
        let exe = PathBuf::from(&stripped[..end]);
        let rest = stripped[end + 1..].trim().to_string();
        (exe, rest)
    } else {
        let mut parts = s.split_whitespace();
        let exe = PathBuf::from(parts.next()?);
        let rest = parts.collect::<Vec<_>>().join(" ");
        (exe, rest)
    };
    let args: Vec<String> = if rest.is_empty() {
        Vec::new()
    } else {
        // Keep simple: one token groups for /SILENT /VERYSILENT etc.
        rest.split_whitespace().map(|t| t.to_string()).collect()
    };
    Some((exe, args))
}

#[cfg(windows)]
fn find_uninstaller_from_registry() -> Option<UninstallerCmd> {
    // PowerShell is more reliable than adding a winreg dependency.
    let script = r#"
$ErrorActionPreference='SilentlyContinue'
$paths = @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
Get-ItemProperty $paths |
  Where-Object {
    $_.DisplayName -match 'Cursor' -or
    $_.InstallLocation -match '(?i)\\cursor\\?$' -or
    $_.UninstallString -match '(?i)cursor'
  } |
  ForEach-Object {
    $quiet = $_.QuietUninstallString
    $plain = $_.UninstallString
    if ($quiet) { "QUIET|$quiet" }
    elseif ($plain) { "PLAIN|$plain" }
  }
"#;
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (kind, rest) = if let Some(r) = line.strip_prefix("QUIET|") {
            ("registry-quiet", r)
        } else if let Some(r) = line.strip_prefix("PLAIN|") {
            ("registry", r)
        } else {
            continue;
        };
        if let Some((exe, args)) = parse_uninstall_command_line(rest) {
            if exe.is_file() {
                return Some(UninstallerCmd {
                    exe,
                    args,
                    source: kind,
                });
            }
        }
    }
    None
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
        if !lower.ends_with(".exe") {
            continue;
        }
        // Match Uninstall.exe, *uninstall*.exe, and Inno Setup unins000.exe / unins001.exe.
        let score = if lower == "uninstall.exe" {
            0
        } else if lower.starts_with("uninstall") {
            1
        } else if lower.contains("uninstall") {
            2
        } else if lower.starts_with("unins") && lower.chars().nth(5).is_some_and(|c| c.is_ascii_digit())
        {
            // unins000.exe
            3
        } else if lower.starts_with("unins") {
            4
        } else {
            continue;
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

fn dedupe_paths(mut out: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    out.retain(|p| {
        if seen.iter().any(|s| paths_equal(s.as_path(), p.as_path())) {
            false
        } else {
            seen.push(p.clone());
            true
        }
    });
    out
}

fn push_cli_leftovers(out: &mut Vec<PathBuf>) {
    #[cfg(windows)]
    {
        if let Some(home) = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
        {
            push_if_exists(out, home.join(".cursor"));
            push_if_exists(out, home.join(".cursor-tutor"));
            push_if_exists(out, home.join(".cursor.json"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            push_if_exists(out, local.join("cursor-agent"));
        }
    }

    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            push_if_exists(out, home.join(".cursor"));
            push_if_exists(out, home.join(".cursor-tutor"));
            push_if_exists(out, home.join(".cursor.json"));
        }
    }
}

fn push_ide_leftovers(out: &mut Vec<PathBuf>) {
    #[cfg(windows)]
    {
        // System installs (common): C:\Program Files\cursor
        for dir in windows_install_dirs() {
            // Only keep Program Files / Program Files (x86) install roots here;
            // LOCALAPPDATA\Programs is also returned by windows_install_dirs — include those too.
            push_if_exists(out, dir);
        }
        if let Some(roaming) = std::env::var_os("APPDATA").map(PathBuf::from) {
            push_if_exists(out, roaming.join("Cursor"));
            let sm = roaming
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            collect_start_menu_cursor(&sm, out);
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            push_if_exists(out, local.join("cursor-updater"));
            if let Ok(entries) = std::fs::read_dir(&local) {
                for ent in entries.flatten() {
                    let name = ent.file_name();
                    let n = name.to_string_lossy();
                    let lower = n.to_ascii_lowercase();
                    // CLI install lives here — skip when wiping IDE only.
                    if lower == "cursor-agent" {
                        continue;
                    }
                    if lower.starts_with("cursor") {
                        push_if_exists(out, ent.path());
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        push_if_exists(out, PathBuf::from("/Applications/Cursor.app"));
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let support = home.join("Library").join("Application Support");
            push_if_exists(out, support.join("Cursor"));
            push_if_exists(out, support.join("Caches").join("cursor-updater"));
            let caches = home.join("Library").join("Caches");
            let td = "com.todesktop.230313mzl4w4u92";
            push_if_exists(out, caches.join(td));
            push_if_exists(out, caches.join(format!("{td}.ShipIt")));
            push_if_exists(
                out,
                home.join("Library").join("HTTPStorages").join(td),
            );
            push_if_exists(out, home.join("Library").join("Logs").join("Cursor"));
            push_if_exists(
                out,
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
                        push_if_exists(out, ent.path());
                    }
                }
            }
            push_if_exists(
                out,
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
                        push_if_exists(out, ent.path());
                    }
                }
            }
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            push_if_exists(out, config.join("Cursor"));
        }
        for cand in [
            PathBuf::from("/usr/bin/cursor"),
            PathBuf::from("/usr/local/bin/cursor"),
            PathBuf::from("/opt/cursor"),
        ] {
            push_if_exists(out, cand);
        }
    }
}

/// Known leftover / install paths that still exist on disk for the given scope.
pub fn leftover_paths(scope: DataScope) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if scope.includes_ide() {
        push_ide_leftovers(&mut out);
    }
    if scope.includes_cli() {
        push_cli_leftovers(&mut out);
    }
    dedupe_paths(out)
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
    let q = crate::elevate::powershell_quote(&agent.display().to_string());
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

fn uninstall_running_label(scope: DataScope) -> &'static str {
    match scope {
        DataScope::Ide => t(Msg::UninstallRunningIde),
        DataScope::Cli => t(Msg::UninstallRunningCli),
        DataScope::Both => t(Msg::UninstallRunning),
    }
}

fn uninstall_done_label(scope: DataScope) -> &'static str {
    match scope {
        DataScope::Ide => t(Msg::UninstallDoneIde),
        DataScope::Cli => t(Msg::UninstallDoneCli),
        DataScope::Both => t(Msg::UninstallDone),
    }
}

/// Dry-run plan: what uninstall would do (never deletes).
#[derive(Debug, Clone)]
pub struct UninstallPlan {
    pub scope: DataScope,
    pub uninstaller: Option<UninstallerCmd>,
    pub leftovers: Vec<PathBuf>,
    pub will_clean_path: bool,
    pub needs_admin: bool,
    pub notes: Vec<String>,
}

/// Build an uninstall plan for verification (read-only).
pub fn plan_uninstall(scope: DataScope) -> UninstallPlan {
    let mut notes = Vec::new();
    let uninstaller = if scope.includes_ide() {
        #[cfg(windows)]
        {
            let cmd = find_windows_uninstaller_cmd();
            if cmd.is_none() {
                notes.push(t(Msg::UninstallNoUninstaller).to_string());
            }
            cmd
        }
        #[cfg(not(windows))]
        {
            notes.push(t(Msg::UninstallNoUninstaller).to_string());
            None
        }
    } else {
        None
    };
    let leftovers = leftover_paths(scope);
    if leftovers.is_empty() {
        notes.push(t(Msg::UninstallPlanNoLeftovers).to_string());
    }
    UninstallPlan {
        scope,
        uninstaller,
        leftovers,
        will_clean_path: {
            #[cfg(windows)]
            {
                scope.includes_cli()
            }
            #[cfg(not(windows))]
            {
                false
            }
        },
        needs_admin: needs_elevation(scope),
        notes,
    }
}

/// Print a dry-run uninstall plan (no deletions).
pub fn print_uninstall_plan(scope: DataScope) {
    let plan = plan_uninstall(scope);
    println!();
    println!("  {}", t(Msg::UninstallPlanTitle));
    println!(
        "  {} {}",
        t(Msg::UninstallPlanScope),
        i18n::scope_short(plan.scope)
    );
    if plan.needs_admin {
        println!("  ⚠ {}", t(Msg::UninstallPlanNeedsAdmin));
    }
    match &plan.uninstaller {
        Some(cmd) => {
            println!("  ✔ {}: {}", t(Msg::UninstallPlanUninstaller), cmd.display_line());
        }
        None if scope.includes_ide() => {
            println!("  ⚠ {}: —", t(Msg::UninstallPlanUninstaller));
        }
        None => {}
    }
    println!(
        "  {}: {}",
        t(Msg::UninstallPlanLeftovers),
        plan.leftovers.len()
    );
    for p in &plan.leftovers {
        println!("    · {}", p.display());
    }
    if plan.will_clean_path {
        println!("  · {}", t(Msg::UninstallStepPath));
    }
    for note in &plan.notes {
        println!("  ⚠ {note}");
    }
    println!("  {}", t(Msg::UninstallPlanDryRun));
    println!();
}

/// Uninstall for `scope`: force-quit → official uninstaller (Windows IDE) → wipe leftovers → PATH.
pub fn run_full_uninstall(scope: DataScope, mut progress: impl FnMut(&str)) -> UninstallReport {
    let mut report = UninstallReport::default();

    progress(i18n::force_quitting_label(scope));
    if !force_quit_scope(Editor::Cursor, scope, true) {
        report.notes.push(t(Msg::ForceQuitFail).to_string());
    }

    #[cfg(windows)]
    if scope.includes_ide() {
        if let Some(cmd) = find_windows_uninstaller_cmd() {
            progress(&format!(
                "{} — {}",
                t(Msg::UninstallStepUninstaller),
                cmd.display_line()
            ));
            report.uninstaller_ran = true;
            let mut c = Command::new(&cmd.exe);
            c.args(&cmd.args);
            match c.status() {
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
    for path in leftover_paths(scope) {
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
    if scope.includes_cli() {
        progress(t(Msg::UninstallStepPath));
        report.path_cleaned = clean_cursor_agent_user_path();
    }

    report
}

/// CLI / elevated entry: print progress to stdout. Returns true when fully successful.
pub fn run_uninstall_cli(scope: DataScope) -> bool {
    println!();
    println!("  {}", uninstall_running_label(scope));
    println!();
    let report = run_full_uninstall(scope, |msg| {
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
        println!("  ✔ {}", uninstall_done_label(scope));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_quoted_quiet_uninstall_string() {
        let (exe, args) = parse_uninstall_command_line(
            r#""C:\Program Files\cursor\unins000.exe" /SILENT"#,
        )
        .unwrap();
        assert_eq!(exe, PathBuf::from(r"C:\Program Files\cursor\unins000.exe"));
        assert_eq!(args, vec!["/SILENT".to_string()]);
    }

    #[test]
    fn parse_plain_uninstall_string() {
        let (exe, args) =
            parse_uninstall_command_line(r"C:\Apps\cursor\Uninstall.exe").unwrap();
        assert_eq!(exe, PathBuf::from(r"C:\Apps\cursor\Uninstall.exe"));
        assert!(args.is_empty());
    }

    #[test]
    fn find_inno_unins000_in_dir() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("cursor-unins-test-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        let unins = dir.join("unins000.exe");
        fs::write(&unins, b"").unwrap();
        fs::write(dir.join("Cursor.exe"), b"").unwrap();

        #[cfg(windows)]
        {
            let found = find_uninstaller_in_dir(&dir).unwrap();
            assert_eq!(found, unins);
            assert!(is_inno_unins(&found));
        }
        #[cfg(not(windows))]
        {
            // parse helper still covered above
            let _ = unins;
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_prefers_uninstall_exe_over_unins() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("cursor-unins-pref-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("unins000.exe"), b"").unwrap();
        let classic = dir.join("Uninstall.exe");
        fs::write(&classic, b"").unwrap();

        #[cfg(windows)]
        {
            let found = find_uninstaller_in_dir(&dir).unwrap();
            assert_eq!(found, classic);
        }

        let _ = fs::remove_dir_all(&dir);
    }
}
