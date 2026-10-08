use crate::fsutil::{
    path_size, probe_path_unlocked, remove_best_effort, unlock_probe_paths,
};
use crate::paths::{clean_targets, is_safe_rel_path, CleanTarget, Editor};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Clone)]
pub struct TargetStat {
    pub target: CleanTarget,
    pub bytes: u64,
}

#[cfg(windows)]
fn windows_images(_editor: Editor) -> &'static [&'static str] {
    &["Cursor.exe"]
}

#[cfg(windows)]
fn csv_first_field(line: &str) -> Option<String> {
    let line = line.trim();
    if line.starts_with('"') {
        let rest = &line[1..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    } else {
        Some(line.split(',').next()?.trim().to_string())
    }
}

/// Exact image-name match via `tasklist /FI` — avoids false positives like
/// `cursor-cleanup.exe` matching a loose `"cursor"` substring search.
#[cfg(windows)]
fn tasklist_has_image(image: &str) -> bool {
    let out = match Command::new("tasklist")
        .args(["/FO", "CSV", "/NH", "/FI", &format!("IMAGENAME eq {image}")])
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|line| {
            let line = line.trim();
            !line.is_empty()
                && !line.starts_with("INFO:")
                && csv_first_field(line)
                    .map(|name| name.eq_ignore_ascii_case(image))
                    .unwrap_or(false)
        })
}

#[cfg(windows)]
fn is_any_image_running(images: &[&str]) -> bool {
    images.iter().any(|img| tasklist_has_image(img))
}

#[cfg(windows)]
fn kill_windows_images(images: &[&str]) {
    for img in images {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", img, "/T"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

pub fn is_editor_running(editor: Editor) -> bool {
    #[cfg(windows)]
    {
        return is_any_image_running(windows_images(editor));
    }

    #[cfg(not(windows))]
    {
        let names = editor.process_names();
        // pgrep -x may not exist everywhere; fall back to ps
        for n in names {
            if Command::new("pgrep")
                .arg("-x")
                .arg(n)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                return true;
            }
        }
        if let Ok(out) = Command::new("ps").arg("-A").arg("-o").arg("comm=").output() {
            let text = String::from_utf8_lossy(&out.stdout);
            return names.iter().any(|n| {
                text.lines().any(|line| {
                    let line = line.trim();
                    // Exact match only — `ends_with("code")` false-positives
                    // binaries like `vscode` / `encode`.
                    line.eq_ignore_ascii_case(n)
                })
            });
        }
        false
    }
}

/// Wait until Cursor data files are unlocked (rename probe), up to ~20s.
/// Missing paths count as unlocked. Returns false on timeout.
pub fn wait_until_data_unlocked(editor: Editor, quiet: bool) -> bool {
    let Some(root) = editor.data_dir() else {
        return true;
    };
    if !root.exists() {
        return true;
    }
    let probes = unlock_probe_paths(&root);
    // Brief settle even when the process was already gone.
    std::thread::sleep(Duration::from_millis(500));
    for i in 0..40 {
        if probes.iter().all(|p| probe_path_unlocked(p)) {
            return true;
        }
        if !quiet {
            let _ = io::stdout().write_all(b".");
            let _ = io::stdout().flush();
        }
        if i + 1 < 40 {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    probes.iter().all(|p| probe_path_unlocked(p))
}

/// Force-quit editor processes, wait until gone, then wait for data files to unlock.
/// Returns true only when the process is gone and critical paths are writable.
/// `quiet`: suppress progress dots on stdout (use in TUI alternate screen).
pub fn force_quit_editor(editor: Editor, quiet: bool) -> bool {
    if is_editor_running(editor) {
        #[cfg(windows)]
        kill_windows_images(windows_images(editor));

        #[cfg(not(windows))]
        {
            let names = editor.process_names();
            for n in names {
                let _ = Command::new("pkill")
                    .args(["-9", "-x", n])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                let _ = Command::new("killall")
                    .args(["-9", n])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }

        // Poll until process list no longer shows the editor (up to ~10s)
        let mut gone = false;
        for i in 0..20 {
            std::thread::sleep(Duration::from_millis(500));
            if !quiet {
                let _ = io::stdout().write_all(b".");
                let _ = io::stdout().flush();
            }
            if !is_editor_running(editor) {
                gone = true;
                break;
            }
            // Re-issue kill every ~2s in case child processes respawned briefly
            if i > 0 && i % 4 == 0 {
                #[cfg(windows)]
                kill_windows_images(windows_images(editor));
            }
        }

        if !gone {
            return false;
        }
    }

    if is_editor_running(editor) {
        return false;
    }

    wait_until_data_unlocked(editor, quiet)
}

/// Candidate install paths for Cursor on Windows under a given LOCALAPPDATA root.
pub fn windows_cursor_candidates(local_app_data: &Path) -> Vec<PathBuf> {
    let programs = local_app_data.join("Programs");
    vec![
        programs.join("cursor").join("Cursor.exe"),
        programs.join("Cursor").join("Cursor.exe"),
    ]
}

#[cfg(windows)]
fn where_first(image: &str) -> Option<PathBuf> {
    let out = Command::new("where")
        .arg(image)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

/// Resolve the Cursor executable / app bundle path for relaunch.
pub fn resolve_editor_exe(editor: Editor) -> Option<PathBuf> {
    let _ = editor;
    #[cfg(windows)]
    {
        if let Some(base) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            for cand in windows_cursor_candidates(&base) {
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
        return where_first("Cursor.exe").or_else(|| where_first("cursor.cmd"));
    }

    #[cfg(target_os = "macos")]
    {
        let app = PathBuf::from("/Applications/Cursor.app");
        if app.is_dir() {
            return Some(app);
        }
        // Fallback: `mdfind` / which
        if let Ok(out) = Command::new("which")
            .arg("cursor")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from);
                if let Some(p) = p {
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for name in ["cursor", "Cursor"] {
            if let Ok(out) = Command::new("which")
                .arg(name)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
            {
                if out.status.success() {
                    if let Some(p) = String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .next()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(PathBuf::from)
                    {
                        if p.exists() {
                            return Some(p);
                        }
                    }
                }
            }
        }
        for cand in [
            PathBuf::from("/usr/bin/cursor"),
            PathBuf::from("/usr/local/bin/cursor"),
            PathBuf::from("/opt/cursor/cursor"),
        ] {
            if cand.is_file() {
                return Some(cand);
            }
        }
        None
    }

    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

/// Launch Cursor detached (do not wait). Returns true if spawn succeeded.
pub fn launch_editor(editor: Editor) -> bool {
    #[cfg(windows)]
    {
        let Some(exe) = resolve_editor_exe(editor) else {
            return false;
        };
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
        const FLAGS: u32 = 0x00000008 | 0x00000200;
        use std::os::windows::process::CommandExt;
        Command::new(&exe)
            .creation_flags(FLAGS)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    }

    #[cfg(target_os = "macos")]
    {
        let _ = editor;
        // Prefer open -a so we get a proper GUI session even if only the .app exists.
        if Path::new("/Applications/Cursor.app").is_dir() {
            return Command::new("open")
                .args(["-a", "Cursor"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok();
        }
        let Some(exe) = resolve_editor_exe(editor) else {
            return false;
        };
        Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let Some(exe) = resolve_editor_exe(editor) else {
            return false;
        };
        Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    }

    #[cfg(not(any(windows, unix)))]
    {
        let _ = editor;
        false
    }
}

pub fn collect_stats(editor: Editor) -> Option<(PathBuf, u64, Vec<TargetStat>)> {
    let root = editor.data_dir()?;
    let mut stats = Vec::new();
    for target in clean_targets() {
        let mut bytes = 0u64;
        for rel in target.rel_paths {
            bytes += path_size(&root.join(rel));
        }
        stats.push(TargetStat { target, bytes });
    }
    // Sum known cleanup targets only — avoids walking the entire app data tree.
    let total: u64 = stats.iter().map(|s| s.bytes).sum();
    Some((root, total, stats))
}

pub struct CleanResult {
    pub freed: u64,
    pub errors: usize,
    /// Bytes still present under the target paths after cleanup.
    pub remaining: u64,
}

/// Join `root`/`rel` only when `rel` stays under `root` (rejects `..` / absolute).
fn join_under_root(root: &std::path::Path, rel: &str) -> Option<std::path::PathBuf> {
    if !is_safe_rel_path(rel) {
        return None;
    }
    let joined = root.join(rel);
    // Component-wise: joined must start with root (after normalize of `.`).
    let root_c: Vec<_> = root.components().collect();
    let join_c: Vec<_> = joined.components().collect();
    if join_c.len() < root_c.len() {
        return None;
    }
    if join_c[..root_c.len()] != root_c[..] {
        return None;
    }
    Some(joined)
}

pub fn run_target(root: &std::path::Path, target: &CleanTarget) -> CleanResult {
    let mut freed = 0u64;
    let mut errors = 0usize;
    for rel in target.rel_paths {
        let Some(path) = join_under_root(root, rel) else {
            errors = errors.saturating_add(1);
            continue;
        };
        let (f, e) = remove_best_effort(&path);
        freed += f;
        errors += e;
    }
    let mut remaining = 0u64;
    for rel in target.rel_paths {
        if let Some(path) = join_under_root(root, rel) {
            remaining += path_size(&path);
        }
    }
    if remaining > 0 {
        // Treat leftover data as a failure so the UI does not show pure success.
        errors = errors.saturating_add(1);
    }
    CleanResult {
        freed,
        errors,
        remaining,
    }
}

#[cfg(test)]
mod tests {
    use super::{join_under_root, run_target, windows_cursor_candidates};
    use crate::paths::{CleanTarget, Risk, TargetKind};
    use std::fs;
    use std::path::PathBuf;

    #[cfg(windows)]
    use super::csv_first_field;

    #[test]
    fn join_under_root_rejects_traversal() {
        let root = std::env::temp_dir().join(format!(
            "cursor-cleanup-root-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&root);
        assert!(join_under_root(&root, "User/History").is_some());
        assert!(join_under_root(&root, "../outside").is_none());
        assert!(join_under_root(&root, "User/../../outside").is_none());
        assert!(join_under_root(&root, "/abs").is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn run_target_skips_unsafe_rel_paths() {
        // Unique stamp so parallel tests (e.g. backup path_under_backups_*) cannot
        // remove_dir_all the same temp folder mid-assert.
        let stamp = format!(
            "guard-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let root = std::env::temp_dir().join(format!("cursor-cleanup-{stamp}"));
        let outside = std::env::temp_dir().join(format!("cursor-cleanup-{stamp}-out"));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("keep.txt");
        fs::write(&victim, b"safe").unwrap();
        let evil = CleanTarget {
            kind: TargetKind::CachedData,
            risk: Risk::Safe,
            rel_paths: &["../cursor-cleanup-must-not-escape"],
        };
        let result = run_target(&root, &evil);
        assert!(result.errors > 0);
        assert!(victim.exists(), "must not delete outside data root");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn windows_cursor_candidates_order() {
        // Build expected paths with the same joins so separators match on every OS
        // (hard-coded `\` strings only equal on Windows).
        let base = PathBuf::from("local-app-data");
        let cands = windows_cursor_candidates(&base);
        assert_eq!(
            cands,
            vec![
                base.join("Programs").join("cursor").join("Cursor.exe"),
                base.join("Programs").join("Cursor").join("Cursor.exe"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn csv_first_field_quoted() {
        assert_eq!(
            csv_first_field(r#""Code.exe","1234","Session Name","0","12 K""#).as_deref(),
            Some("Code.exe")
        );
    }

    #[cfg(windows)]
    #[test]
    fn csv_first_field_unquoted() {
        assert_eq!(
            csv_first_field("Code.exe,1234,Console,1,10 K").as_deref(),
            Some("Code.exe")
        );
    }

    #[cfg(windows)]
    #[test]
    fn csv_first_field_ignores_info() {
        // Empty / INFO lines are filtered by callers; parser still returns first field.
        assert_eq!(csv_first_field("INFO: No tasks").as_deref(), Some("INFO: No tasks"));
    }
}
