use crate::cleanup::{is_editor_running, wait_until_data_unlocked};
use crate::cursor_version;
use crate::fsutil::{
    clear_state_sidecars, copy_best_effort, copy_verified, fmt_size, path_size, sizes_match,
};
use crate::paths::{
    agent_cli_auth_rel_paths, config_rel_paths, exe_dir, timestamp, DataScope, Editor,
};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MANIFEST: &str = "manifest.txt";
const MAX_NAME_CHARS: usize = 64;

#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub app: String,
    /// Display / folder tag (custom label or timestamp).
    pub name: String,
    pub created: String,
    pub include_state: bool,
    pub bytes: u64,
    /// Cursor app version recorded at backup time (`None` for older backups).
    pub cursor_version: Option<String>,
}

impl BackupInfo {
    pub fn label(&self) -> String {
        let state = if self.include_state { "+state" } else { "config" };
        let title = if self.name != self.created && !self.name.is_empty() {
            format!("{} ({})", self.name, self.created)
        } else {
            self.name.clone()
        };
        let ver = self
            .cursor_version
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|v| format!("  ·  Cursor {v}"))
            .unwrap_or_default();
        format!(
            "{}  ·  {}  ·  {}  ·  {}{}",
            self.app,
            title,
            state,
            fmt_size(self.bytes),
            ver
        )
    }
}

/// Sanitize a user-supplied backup name for use as a folder segment.
/// Returns `None` when empty / unusable → caller should use the default timestamp.
pub fn sanitize_backup_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut s: String = trimmed
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '-'
            } else {
                c
            }
        })
        .collect();

    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s = s
        .trim_matches(|c: char| c == '-' || c == '.' || c == ' ')
        .chars()
        .take(MAX_NAME_CHARS)
        .collect::<String>()
        .trim_matches(|c: char| c == '-' || c == '.' || c == ' ')
        .to_string();

    if s.is_empty() || s == "." || s == ".." {
        return None;
    }

    // Avoid Windows reserved device names as the whole folder tag.
    let stem = s.split('.').next().unwrap_or(&s);
    let reserved = matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        s.push_str("-bak");
    }

    Some(s)
}

fn allocate_backup_dir(base: &Path, editor: Editor, folder_tag: &str) -> io::Result<PathBuf> {
    let dir = base.join(format!("{}-backup-{}", editor.slug(), folder_tag));
    if !dir.exists() {
        return Ok(dir);
    }
    let mut n = 2u32;
    loop {
        let cand = base.join(format!("{}-backup-{}-{}", editor.slug(), folder_tag, n));
        if !cand.exists() {
            return Ok(cand);
        }
        n = n.saturating_add(1);
        if n > 10_000 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "backup directory collision",
            ));
        }
    }
}

/// Backups live next to the exe: `{exe_dir}/backups/`
pub fn backups_root() -> Option<PathBuf> {
    Some(exe_dir()?.join("backups"))
}

fn write_manifest(
    dir: &Path,
    editor: Editor,
    include_state: bool,
    created: &str,
    name: &str,
    cursor_version: Option<&str>,
) -> io::Result<()> {
    let mut f = File::create(dir.join(MANIFEST))?;
    writeln!(f, "app={}", editor.slug())?;
    writeln!(f, "created={created}")?;
    writeln!(f, "name={name}")?;
    writeln!(f, "include_state={}", if include_state { "true" } else { "false" })?;
    writeln!(f, "version={}", env!("CARGO_PKG_VERSION"))?;
    if let Some(v) = cursor_version.map(str::trim).filter(|s| !s.is_empty()) {
        writeln!(f, "cursor_version={v}")?;
    }
    Ok(())
}

fn read_manifest(dir: &Path) -> Option<(String, String, String, bool, Option<String>)> {
    let text = fs::read_to_string(dir.join(MANIFEST)).ok()?;
    let mut app = String::new();
    let mut created = String::new();
    let mut name = String::new();
    let mut include_state = false;
    let mut cursor_version = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("app=") {
            app = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("created=") {
            created = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("name=") {
            name = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("include_state=") {
            include_state = v.trim().eq_ignore_ascii_case("true");
        } else if let Some(v) = line.strip_prefix("cursor_version=") {
            let v = v.trim();
            if !v.is_empty() {
                cursor_version = Some(v.to_string());
            }
        }
    }
    if app.is_empty() {
        app = dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    if created.is_empty() {
        created = dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "?".into());
    }
    if name.is_empty() {
        name = created.clone();
    }
    Some((app, created, name, include_state, cursor_version))
}

/// Create a backup under `…/backups/{slug}-backup-{name|timestamp}/`.
/// `custom_name`: optional label; empty / invalid → default timestamp folder name.
#[allow(dead_code)]
pub fn create_backup(
    editor: Editor,
    include_state: bool,
    custom_name: Option<&str>,
) -> io::Result<BackupInfo> {
    create_backup_for(editor, DataScope::Both, include_state, custom_name)
}

/// Backup contents depend on `scope`:
/// - IDE: settings (+ `state.vscdb` when `include_state`)
/// - CLI: `auth.json` (+ `.bak`)
/// - Both: IDE items above, plus `auth.json` when `include_state`
pub fn create_backup_for(
    editor: Editor,
    scope: DataScope,
    include_state: bool,
    custom_name: Option<&str>,
) -> io::Result<BackupInfo> {
    let root = editor
        .data_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "data dir"))?;
    if scope.includes_ide() && !root.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "app data missing"));
    }
    if scope == DataScope::Cli && !root.exists() {
        // auth.json still lives under the IDE data dir.
        return Err(io::Error::new(io::ErrorKind::NotFound, "app data missing"));
    }

    let base = backups_root().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "backup root unavailable")
    })?;
    fs::create_dir_all(&base)?;

    let created = timestamp();
    let display_name = custom_name
        .and_then(sanitize_backup_name)
        .unwrap_or_else(|| created.clone());
    let dir = allocate_backup_dir(&base, editor, &display_name)?;
    fs::create_dir_all(&dir)?;

    if scope.includes_ide() {
        for rel in config_rel_paths() {
            let src = root.join(rel);
            if !src.exists() {
                continue;
            }
            let file_name = Path::new(rel)
                .file_name()
                .map(|s| s.to_os_string())
                .unwrap_or_default();
            copy_best_effort(&src, &dir.join(file_name))?;
        }
        if include_state {
            let state = root.join("User/globalStorage/state.vscdb");
            if state.exists() {
                copy_best_effort(&state, &dir.join("state.vscdb"))?;
            }
        }
    }

    // CLI login: always for CLI-only; with full backup when Both + include_state.
    let copy_auth = matches!(scope, DataScope::Cli)
        || (matches!(scope, DataScope::Both) && include_state);
    if copy_auth {
        for rel in agent_cli_auth_rel_paths() {
            let src = root.join(rel);
            if !src.exists() {
                continue;
            }
            let file_name = Path::new(rel)
                .file_name()
                .map(|s| s.to_os_string())
                .unwrap_or_default();
            copy_best_effort(&src, &dir.join(file_name))?;
        }
    }

    // Manifest `include_state` means "fuller backup" for restore UX.
    let manifest_state = match scope {
        DataScope::Cli => true,
        DataScope::Ide | DataScope::Both => include_state,
    };

    let cursor_version = cursor_version::detect_installed();
    write_manifest(
        &dir,
        editor,
        manifest_state,
        &created,
        &display_name,
        cursor_version.as_deref(),
    )?;

    Ok(BackupInfo {
        path: dir.clone(),
        app: editor.slug().to_string(),
        name: display_name,
        created,
        include_state: manifest_state,
        bytes: path_size(&dir),
        cursor_version,
    })
}

pub fn list_backups(filter_app: Option<Editor>) -> Vec<BackupInfo> {
    let Some(base) = backups_root() else {
        return Vec::new();
    };
    if !base.exists() {
        return Vec::new();
    }

    let filter = filter_app.map(|e| e.slug().to_string());
    let mut out = Vec::new();

    if let Ok(rd) = fs::read_dir(&base) {
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // Prefer manifest; also accept folders that look like backups
            let (app, created, name, include_state, cursor_version) = match read_manifest(&path) {
                Some(m) => m,
                None => {
                    let folder = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    // cursor-backup-xxx (legacy vscode-* folders are ignored)
                    if !folder.starts_with("cursor-") {
                        continue;
                    }
                    let tag = folder
                        .strip_prefix("cursor-backup-")
                        .unwrap_or(&folder)
                        .to_string();
                    (
                        "cursor".into(),
                        folder.clone(),
                        tag,
                        path.join("state.vscdb").exists(),
                        None,
                    )
                }
            };

            // Cursor-only tool — skip any non-cursor backups (e.g. old vscode-*).
            if app != "cursor" {
                continue;
            }
            if let Some(ref f) = filter {
                if &app != f {
                    continue;
                }
            }

            out.push(BackupInfo {
                bytes: path_size(&path),
                path,
                app,
                name,
                created,
                include_state,
                cursor_version,
            });
        }
    }

    // newest first (by created string descending)
    out.sort_by(|a, b| b.created.cmp(&a.created));
    out
}

#[cfg(test)]
mod tests {
    use super::{
        create_backup_for, delete_backup, path_under_backups_root, read_manifest,
        sanitize_backup_name,
    };
    use crate::paths::{DataScope, Editor};
    use std::fs;
    use std::path::Path;

    #[test]
    fn sanitize_empty_is_none() {
        assert!(sanitize_backup_name("").is_none());
        assert!(sanitize_backup_name("   ").is_none());
        assert!(sanitize_backup_name("???***").is_none());
    }

    #[test]
    fn sanitize_keeps_unicode_and_strips_illegal() {
        assert_eq!(
            sanitize_backup_name("  深度清理前  ").as_deref(),
            Some("深度清理前")
        );
        assert_eq!(
            sanitize_backup_name("before:deep/clean").as_deref(),
            Some("before-deep-clean")
        );
    }

    #[test]
    fn sanitize_reserved_windows_names() {
        assert_eq!(sanitize_backup_name("CON").as_deref(), Some("CON-bak"));
        assert_eq!(sanitize_backup_name("nul.txt").as_deref(), Some("nul.txt-bak"));
    }

    #[test]
    fn old_manifest_without_cursor_version_still_loads() {
        let stamp = format!(
            "oldman-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let dir = std::env::temp_dir().join(format!("cursor-cleanup-{stamp}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("manifest.txt"),
            "app=cursor\ncreated=2026-01-01-00-00-00-000\nname=legacy\ninclude_state=true\nversion=0.4.3\n",
        )
        .unwrap();
        let m = read_manifest(&dir).expect("legacy manifest");
        assert_eq!(m.0, "cursor");
        assert_eq!(m.2, "legacy");
        assert!(m.3);
        assert_eq!(m.4, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_under_backups_rejects_root_and_outside() {
        let stamp = format!("bak-{}-{}", std::process::id(), std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0));
        let root = std::env::temp_dir().join(format!("cursor-cleanup-{stamp}-root"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let inside = root.join("cursor-backup-x");
        fs::create_dir_all(&inside).unwrap();
        assert!(path_under_backups_root(&inside, &root).unwrap());
        assert!(!path_under_backups_root(&root, &root).unwrap());
        let outside = std::env::temp_dir().join(format!("cursor-cleanup-{stamp}-out"));
        fs::create_dir_all(&outside).unwrap();
        assert!(!path_under_backups_root(&outside, &root).unwrap());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    /// Serialize tests that temporarily redirect APPDATA / HOME.
    fn with_fake_cursor_home<R>(f: impl FnOnce(&Path) -> R) -> R {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        let stamp = format!(
            "scope-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let base = std::env::temp_dir().join(format!("cursor-cleanup-{stamp}"));
        let appdata = base.join("AppData");
        let home = base.join("Home");
        let cursor = appdata.join("Cursor");
        let user = cursor.join("User");
        fs::create_dir_all(user.join("globalStorage")).unwrap();
        fs::create_dir_all(user.join("snippets")).unwrap();
        fs::create_dir_all(home.join(".cursor")).unwrap();
        fs::write(user.join("settings.json"), b"{\"editor.fontSize\":14}").unwrap();
        fs::write(user.join("keybindings.json"), b"[]").unwrap();
        fs::write(
            user.join("globalStorage").join("state.vscdb"),
            b"fake-state",
        )
        .unwrap();
        fs::write(cursor.join("auth.json"), b"{\"accessToken\":\"tok\"}").unwrap();
        fs::write(cursor.join("auth.json.bak"), b"{\"accessToken\":\"old\"}").unwrap();

        #[cfg(windows)]
        {
            let prev_app = std::env::var_os("APPDATA");
            let prev_home = std::env::var_os("USERPROFILE");
            std::env::set_var("APPDATA", &appdata);
            std::env::set_var("USERPROFILE", &home);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&cursor)));
            match prev_app {
                Some(v) => std::env::set_var("APPDATA", v),
                None => std::env::remove_var("APPDATA"),
            }
            match prev_home {
                Some(v) => std::env::set_var("USERPROFILE", v),
                None => std::env::remove_var("USERPROFILE"),
            }
            let _ = fs::remove_dir_all(&base);
            match result {
                Ok(v) => v,
                Err(p) => std::panic::resume_unwind(p),
            }
        }
        #[cfg(not(windows))]
        {
            let prev_home = std::env::var_os("HOME");
            let prev_xdg = std::env::var_os("XDG_CONFIG_HOME");
            std::env::set_var("HOME", &home);
            // Linux IDE path uses XDG_CONFIG_HOME/Cursor; macOS uses Application Support.
            #[cfg(target_os = "macos")]
            {
                let support = home.join("Library").join("Application Support").join("Cursor");
                fs::create_dir_all(support.join("User").join("globalStorage")).unwrap();
                fs::create_dir_all(support.join("User").join("snippets")).unwrap();
                fs::copy(user.join("settings.json"), support.join("User").join("settings.json")).unwrap();
                fs::copy(user.join("keybindings.json"), support.join("User").join("keybindings.json")).unwrap();
                fs::copy(
                    user.join("globalStorage").join("state.vscdb"),
                    support.join("User").join("globalStorage").join("state.vscdb"),
                )
                .unwrap();
                fs::copy(cursor.join("auth.json"), support.join("auth.json")).unwrap();
                fs::copy(cursor.join("auth.json.bak"), support.join("auth.json.bak")).unwrap();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&support)));
                match prev_home {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
                let _ = fs::remove_dir_all(&base);
                match result {
                    Ok(v) => v,
                    Err(p) => std::panic::resume_unwind(p),
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                std::env::set_var("XDG_CONFIG_HOME", &appdata);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&cursor)));
                match prev_home {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
                match prev_xdg {
                    Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
                    None => std::env::remove_var("XDG_CONFIG_HOME"),
                }
                let _ = fs::remove_dir_all(&base);
                match result {
                    Ok(v) => v,
                    Err(p) => std::panic::resume_unwind(p),
                }
            }
        }
    }

    #[test]
    fn backup_cli_scope_copies_auth_only_then_delete() {
        with_fake_cursor_home(|_cursor| {
            let info = create_backup_for(
                Editor::Cursor,
                DataScope::Cli,
                true,
                Some("unit-cli-scope"),
            )
            .expect("cli backup");
            assert!(info.path.join("auth.json").is_file());
            assert!(info.path.join("auth.json.bak").is_file());
            assert!(!info.path.join("settings.json").exists());
            assert!(!info.path.join("state.vscdb").exists());
            assert!(info.include_state);

            delete_backup(&info).expect("delete cli backup");
            assert!(
                !info.path.exists(),
                "backup folder should be gone after delete"
            );
        });
    }

    #[test]
    fn backup_ide_scope_keeps_auth_out_of_config_only() {
        with_fake_cursor_home(|_cursor| {
            let info = create_backup_for(
                Editor::Cursor,
                DataScope::Ide,
                false,
                Some("unit-ide-config"),
            )
            .expect("ide config backup");
            assert!(info.path.join("settings.json").is_file());
            assert!(!info.path.join("auth.json").exists());
            assert!(!info.path.join("state.vscdb").exists());
            delete_backup(&info).expect("delete ide backup");
            assert!(!info.path.exists());
        });
    }

    #[test]
    fn backup_both_full_includes_state_and_auth() {
        with_fake_cursor_home(|_cursor| {
            let info = create_backup_for(
                Editor::Cursor,
                DataScope::Both,
                true,
                Some("unit-both-full"),
            )
            .expect("both full backup");
            assert!(info.path.join("settings.json").is_file());
            assert!(info.path.join("state.vscdb").is_file());
            assert!(info.path.join("auth.json").is_file());
            delete_backup(&info).expect("delete both backup");
            assert!(!info.path.exists());
        });
    }

    /// Live smoke: copy real CLI auth into backups/, verify, then delete the backup folder.
    /// Does not modify the live `auth.json`.
    #[test]
    fn live_cli_backup_then_delete_smoke() {
        let Some(root) = Editor::Cursor.data_dir() else {
            return;
        };
        if !root.join("auth.json").is_file() {
            return;
        }
        let info = create_backup_for(
            Editor::Cursor,
            DataScope::Cli,
            true,
            Some("live-cli-smoke"),
        )
        .expect("live cli backup");
        assert!(
            info.path.join("auth.json").is_file(),
            "backup must contain auth.json"
        );
        assert!(
            !info.path.join("settings.json").exists(),
            "CLI scope must not copy IDE settings"
        );
        assert!(
            !info.path.join("state.vscdb").exists(),
            "CLI scope must not copy state.vscdb"
        );
        // Live auth must still be present after backup.
        assert!(root.join("auth.json").is_file());
        delete_backup(&info).expect("delete live cli backup");
        assert!(!info.path.exists());
        assert!(
            root.join("auth.json").is_file(),
            "deleting backup must not remove live auth.json"
        );
    }
}

/// True when `path` resolves under `root` (both canonicalized when possible).
fn path_under_backups_root(path: &Path, root: &Path) -> io::Result<bool> {
    if path.as_os_str().is_empty() || path == root {
        return Ok(false);
    }
    let resolved_root = if root.exists() {
        root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
    } else {
        root.to_path_buf()
    };
    let resolved_path = if path.exists() {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    Ok(resolved_path.starts_with(&resolved_root))
}

fn verify_restored(src: &Path, dst: &Path) -> io::Result<()> {
    if !sizes_match(src, dst) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "restore verify failed: size mismatch",
        ));
    }
    Ok(())
}

pub fn restore_backup(info: &BackupInfo) -> io::Result<()> {
    if info.app != "cursor" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unknown app in backup",
        ));
    }
    let editor = Editor::Cursor;
    if is_editor_running(editor) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "cursor still running",
        ));
    }
    if !wait_until_data_unlocked(editor, true) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "data still locked",
        ));
    }
    let root = editor
        .data_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "data dir"))?;

    // Ensure User dirs exist
    fs::create_dir_all(root.join("User"))?;
    let global = root.join("User/globalStorage");
    fs::create_dir_all(&global)?;

    for name in ["settings.json", "keybindings.json"] {
        let src = info.path.join(name);
        if src.exists() {
            let dst = root.join("User").join(name);
            copy_verified(&src, &dst, 10)?;
            verify_restored(&src, &dst)?;
        }
    }

    let snippets_src = info.path.join("snippets");
    if snippets_src.exists() {
        let dest = root.join("User/snippets");
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        copy_verified(&snippets_src, &dest, 10)?;
        verify_restored(&snippets_src, &dest)?;
    }

    let state_src = info.path.join("state.vscdb");
    if state_src.exists() {
        // Stale WAL/SHM after force-kill can make Cursor ignore the restored DB.
        clear_state_sidecars(&global)?;
        let state_dst = global.join("state.vscdb");
        if state_dst.exists() {
            let (_, e) = crate::fsutil::remove_best_effort(&state_dst);
            if e > 0 && state_dst.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "data still locked",
                ));
            }
        }
        clear_state_sidecars(&global)?;
        copy_verified(&state_src, &state_dst, 10)?;
        verify_restored(&state_src, &state_dst)?;
        // Ensure sidecars did not reappear empty / stale from a race.
        clear_state_sidecars(&global)?;
    }

    for name in ["auth.json", "auth.json.bak"] {
        let src = info.path.join(name);
        if src.exists() {
            let dst = root.join(name);
            copy_verified(&src, &dst, 10)?;
            verify_restored(&src, &dst)?;
        }
    }

    Ok(())
}

/// Delete one backup folder. Refuses paths outside `{exe_dir}/backups/`.
pub fn delete_backup(info: &BackupInfo) -> io::Result<()> {
    let root = backups_root().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "backup root unavailable")
    })?;
    let path = &info.path;
    if !path_under_backups_root(path, &root)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to delete outside backups/",
        ));
    }
    if !path.exists() {
        return Ok(());
    }
    fs::remove_dir_all(path)?;
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "delete verify failed: path still exists",
        ));
    }
    Ok(())
}

