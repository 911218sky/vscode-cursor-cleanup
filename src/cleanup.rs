use crate::fsutil::{path_size, remove_best_effort};
use crate::paths::{clean_targets, CleanTarget, Editor};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

pub struct TargetStat {
    pub target: CleanTarget,
    pub bytes: u64,
}

/// Process images whose data dir matches the selected editor.
/// Intentionally excludes VS Code Insiders — cleaning stable `Code` must not
/// kill Insiders (separate data directory).
#[cfg(windows)]
fn windows_images(editor: Editor) -> &'static [&'static str] {
    match editor {
        Editor::Cursor => &["Cursor.exe"],
        Editor::VsCode => &["Code.exe"],
    }
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

/// Force-quit editor processes, wait until gone, then settle for file locks.
/// Returns true if no longer running afterwards.
pub fn force_quit_editor(editor: Editor) -> bool {
    if !is_editor_running(editor) {
        return true;
    }

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
        let _ = io::stdout().write_all(b".");
        let _ = io::stdout().flush();
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

    if gone {
        // Extra settle time so file locks (state.vscdb etc.) are released
        std::thread::sleep(Duration::from_secs(2));
    }

    !is_editor_running(editor)
}

pub fn collect_stats(editor: Editor) -> Option<(PathBuf, u64, Vec<TargetStat>)> {
    let root = editor.data_dir()?;
    let total = path_size(&root);
    let mut stats = Vec::new();
    for target in clean_targets() {
        let mut bytes = 0u64;
        for rel in target.rel_paths {
            bytes += path_size(&root.join(rel));
        }
        stats.push(TargetStat { target, bytes });
    }
    Some((root, total, stats))
}

pub struct CleanResult {
    pub freed: u64,
    pub errors: usize,
}

pub fn run_target(root: &std::path::Path, target: &CleanTarget) -> CleanResult {
    let mut freed = 0u64;
    let mut errors = 0usize;
    for rel in target.rel_paths {
        let (f, e) = remove_best_effort(&root.join(rel));
        freed += f;
        errors += e;
    }
    CleanResult { freed, errors }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::csv_first_field;

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
