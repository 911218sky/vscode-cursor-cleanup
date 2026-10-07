use crate::fsutil::{copy_best_effort, fmt_size, path_size};
use crate::paths::{config_rel_paths, exe_dir, timestamp, Editor};
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
}

impl BackupInfo {
    pub fn label(&self) -> String {
        let state = if self.include_state { "+state" } else { "config" };
        let title = if self.name != self.created && !self.name.is_empty() {
            format!("{} ({})", self.name, self.created)
        } else {
            self.name.clone()
        };
        format!(
            "{}  ·  {}  ·  {}  ·  {}",
            self.app,
            title,
            state,
            fmt_size(self.bytes)
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
) -> io::Result<()> {
    let mut f = File::create(dir.join(MANIFEST))?;
    writeln!(f, "app={}", editor.slug())?;
    writeln!(f, "created={created}")?;
    writeln!(f, "name={name}")?;
    writeln!(f, "include_state={}", if include_state { "true" } else { "false" })?;
    writeln!(f, "version={}", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

fn read_manifest(dir: &Path) -> Option<(String, String, String, bool)> {
    let text = fs::read_to_string(dir.join(MANIFEST)).ok()?;
    let mut app = String::new();
    let mut created = String::new();
    let mut name = String::new();
    let mut include_state = false;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("app=") {
            app = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("created=") {
            created = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("name=") {
            name = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("include_state=") {
            include_state = v.trim().eq_ignore_ascii_case("true");
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
    Some((app, created, name, include_state))
}

/// Create a backup under `…/backups/{slug}-backup-{name|timestamp}/`.
/// `custom_name`: optional label; empty / invalid → default timestamp folder name.
pub fn create_backup(
    editor: Editor,
    include_state: bool,
    custom_name: Option<&str>,
) -> io::Result<BackupInfo> {
    let root = editor
        .data_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "data dir"))?;
    if !root.exists() {
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

    // config files
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

    write_manifest(&dir, editor, include_state, &created, &display_name)?;

    Ok(BackupInfo {
        path: dir.clone(),
        app: editor.slug().to_string(),
        name: display_name,
        created,
        include_state,
        bytes: path_size(&dir),
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
            let (app, created, name, include_state) = match read_manifest(&path) {
                Some(m) => m,
                None => {
                    let folder = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    // cursor-backup-xxx or vscode-backup-xxx
                    let app = if folder.starts_with("cursor-") {
                        "cursor".into()
                    } else if folder.starts_with("vscode-") {
                        "vscode".into()
                    } else {
                        continue;
                    };
                    let tag = folder
                        .strip_prefix("cursor-backup-")
                        .or_else(|| folder.strip_prefix("vscode-backup-"))
                        .unwrap_or(&folder)
                        .to_string();
                    (app, folder.clone(), tag, path.join("state.vscdb").exists())
                }
            };

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
            });
        }
    }

    // newest first (by created string descending)
    out.sort_by(|a, b| b.created.cmp(&a.created));
    out
}

#[cfg(test)]
mod tests {
    use super::sanitize_backup_name;

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

pub fn restore_backup(info: &BackupInfo) -> io::Result<()> {
    let editor = match info.app.as_str() {
        "cursor" => Editor::Cursor,
        "vscode" => Editor::VsCode,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unknown app in backup",
            ))
        }
    };
    let root = editor
        .data_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "data dir"))?;

    // Best-effort snapshot before overwriting live config.
    if root.exists() {
        let _ = create_backup(editor, true, Some("pre-restore"));
    }

    // Ensure User dirs exist
    fs::create_dir_all(root.join("User"))?;
    fs::create_dir_all(root.join("User/globalStorage"))?;

    for name in ["settings.json", "keybindings.json"] {
        let src = info.path.join(name);
        if src.exists() {
            copy_best_effort(&src, &root.join("User").join(name))?;
        }
    }

    let snippets_src = info.path.join("snippets");
    if snippets_src.exists() {
        let dest = root.join("User/snippets");
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        copy_best_effort(&snippets_src, &dest)?;
    }

    let state_src = info.path.join("state.vscdb");
    if state_src.exists() {
        copy_best_effort(&state_src, &root.join("User/globalStorage/state.vscdb"))?;
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
    Ok(())
}

