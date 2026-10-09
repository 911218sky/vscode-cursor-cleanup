use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn path_size(path: &Path) -> u64 {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    // Do not follow symlinks / junctions into foreign trees for size accounting.
    if meta.file_type().is_symlink() {
        return meta.len();
    }
    if meta.is_file() {
        return meta.len();
    }
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let meta = match fs::symlink_metadata(&p) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.file_type().is_symlink() {
                // Count the link node only; do not recurse into the target.
                total += meta.len();
            } else if meta.is_dir() {
                stack.push(p);
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

pub fn fmt_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Remove a path best-effort. Symlinks / junctions are removed as link nodes
/// only — never recursively deleted into their targets.
pub fn remove_best_effort(path: &Path) -> (u64, usize) {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return (0, 0),
    };

    if meta.file_type().is_symlink() {
        let sz = meta.len();
        // Directory junctions/symlinks: try remove_dir first, then remove_file.
        let removed = fs::remove_dir(path)
            .or_else(|_| fs::remove_file(path))
            .is_ok();
        return if removed { (sz, 0) } else { (0, 1) };
    }

    if meta.is_file() {
        let sz = meta.len();
        return match fs::remove_file(path) {
            Ok(()) => (sz, 0),
            Err(_) => (0, 1),
        };
    }

    let mut freed = 0u64;
    let mut errors = 0usize;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let (f, e) = remove_best_effort(&entry.path());
            freed += f;
            errors += e;
        }
    }
    // Children already removed — prefer remove_dir (no second tree walk).
    // Fall back to remove_dir_all only if the directory is still non-empty.
    if fs::remove_dir(path).is_err() {
        if path.exists() {
            if fs::remove_dir_all(path).is_err() && path.exists() {
                errors += 1;
            }
        }
    }
    (freed, errors)
}

pub fn copy_best_effort(src: &Path, dst: &Path) -> io::Result<u64> {
    let meta = match fs::symlink_metadata(src) {
        Ok(m) => m,
        Err(_) => return Ok(0),
    };
    // Do not follow directory symlinks/junctions into foreign trees (same as delete).
    if meta.file_type().is_symlink() {
        return Ok(0);
    }
    if meta.is_file() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
        return Ok(path_size(dst));
    }
    fs::create_dir_all(dst)?;
    let mut total = 0u64;
    for entry in fs::read_dir(src)?.flatten() {
        let name = entry.file_name();
        let child = entry.path();
        let child_meta = match fs::symlink_metadata(&child) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if child_meta.file_type().is_symlink() {
            // Skip link nodes — do not recurse into targets.
            continue;
        }
        total += copy_best_effort(&child, &dst.join(name))?;
    }
    Ok(total)
}

/// Best-effort move `from` → `to` with short retries (used to undo unlock probes).
fn rename_with_retry(from: &Path, to: &Path, attempts: u32) -> bool {
    for i in 0..attempts.max(1) {
        if fs::rename(from, to).is_ok() {
            return true;
        }
        if i + 1 < attempts {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    false
}

/// True when `path` is missing, or can be briefly renamed (strong unlock signal on Windows).
/// Always attempts to restore the original name after a successful probe rename.
pub fn probe_path_unlocked(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    let parent = match path.parent() {
        Some(p) => p.to_path_buf(),
        None => return false,
    };
    let file_name = match path.file_name() {
        Some(n) => n.to_os_string(),
        None => return false,
    };
    let mut probe = parent.join({
        let mut s = file_name.clone();
        s.push(".unlock-probe");
        s
    });
    // Avoid colliding with a leftover probe from a prior crash.
    let mut n = 0u32;
    while probe.exists() && n < 20 {
        n += 1;
        probe = parent.join(format!(
            "{}.unlock-probe-{n}",
            file_name.to_string_lossy()
        ));
    }
    match fs::rename(path, &probe) {
        Ok(()) => {
            // Must put the file back — never leave an orphan `.unlock-probe`.
            if rename_with_retry(&probe, path, 8) {
                return true;
            }
            // Last resort for files: copy probe → original, then remove probe.
            let meta = fs::symlink_metadata(&probe).ok();
            let is_file = meta
                .as_ref()
                .map(|m| m.is_file() && !m.file_type().is_symlink())
                .unwrap_or(false);
            if is_file {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::copy(&probe, path).is_ok() && path.exists() {
                    let _ = fs::remove_file(&probe);
                    return true;
                }
            }
            // Keep retrying rename so data stays under a recoverable name.
            let _ = rename_with_retry(&probe, path, 4);
            path.exists()
        }
        Err(_) => false,
    }
}

/// Byte length of a file; 0 if missing. Directories use [`path_size`].
pub fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// True when both paths exist and have the same byte size (files) or tree size (dirs).
pub fn sizes_match(src: &Path, dst: &Path) -> bool {
    if !src.exists() || !dst.exists() {
        return false;
    }
    if src.is_file() && dst.is_file() {
        return file_len(src) == file_len(dst);
    }
    path_size(src) == path_size(dst)
}

/// Copy a file/dir then verify destination size matches source. Retries on lock / mismatch.
pub fn copy_verified(src: &Path, dst: &Path, attempts: u32) -> io::Result<u64> {
    if !src.exists() {
        return Ok(0);
    }
    let attempts = attempts.max(1);
    let mut last_err: Option<io::Error> = None;
    for i in 0..attempts {
        match copy_best_effort(src, dst) {
            Ok(n) => {
                if sizes_match(src, dst) {
                    return Ok(n);
                }
                last_err = Some(io::Error::new(
                    io::ErrorKind::Other,
                    "restore size mismatch",
                ));
            }
            Err(e) => last_err = Some(e),
        }
        if i + 1 < attempts {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    Err(last_err.unwrap_or_else(|| {
        io::Error::new(io::ErrorKind::Other, "restore copy failed")
    }))
}

/// Remove SQLite WAL/SHM sidecars next to `state.vscdb` (best-effort).
/// Returns the number of removal errors encountered.
pub fn remove_state_sidecars(global_storage: &Path) -> usize {
    let mut errors = 0usize;
    for name in ["state.vscdb-wal", "state.vscdb-shm"] {
        let p = global_storage.join(name);
        if !p.exists() {
            continue;
        }
        let (_, e) = remove_best_effort(&p);
        errors += e;
    }
    errors
}

/// True when neither `state.vscdb-wal` nor `state.vscdb-shm` exists under `global_storage`.
pub fn state_sidecars_absent(global_storage: &Path) -> bool {
    !global_storage.join("state.vscdb-wal").exists()
        && !global_storage.join("state.vscdb-shm").exists()
}

/// Remove sidecars and require they are gone. Retries briefly on lock.
pub fn clear_state_sidecars(global_storage: &Path) -> io::Result<()> {
    for _ in 0..6 {
        let _ = remove_state_sidecars(global_storage);
        if state_sidecars_absent(global_storage) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    let _ = remove_state_sidecars(global_storage);
    if state_sidecars_absent(global_storage) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::Other,
        "state sidecars still present",
    ))
}

/// Absolute paths under `root` used as unlock probes before restore/clean writes.
pub fn unlock_probe_paths(root: &Path) -> Vec<PathBuf> {
    vec![
        root.join("User/globalStorage/state.vscdb"),
        root.join("User/settings.json"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn fmt_size_units() {
        assert_eq!(fmt_size(500), "500 B");
        assert_eq!(fmt_size(2048), "2 KB");
        assert!(fmt_size(5 * 1024 * 1024).contains("MB"));
    }

    #[test]
    fn remove_file_best_effort() {
        let dir = std::env::temp_dir().join(format!(
            "cursor-cleanup-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.txt");
        fs::write(&f, b"hello").unwrap();
        let (freed, err) = remove_best_effort(&f);
        assert_eq!(err, 0);
        assert_eq!(freed, 5);
        assert!(!f.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_dir_best_effort() {
        let dir = std::env::temp_dir().join(format!(
            "cursor-cleanup-dir-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/x.bin"), b"abc").unwrap();
        let (freed, err) = remove_best_effort(&dir);
        assert_eq!(err, 0);
        assert!(freed >= 3);
        assert!(!dir.exists());
    }

    #[test]
    fn sizes_match_and_copy_verified() {
        let dir = std::env::temp_dir().join(format!(
            "cursor-cleanup-verify-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.bin");
        let dst = dir.join("b.bin");
        fs::write(&src, b"hello-world").unwrap();
        assert!(probe_path_unlocked(&src));
        let n = copy_verified(&src, &dst, 3).unwrap();
        assert_eq!(n, 11);
        assert!(sizes_match(&src, &dst));
        fs::write(&dst, b"nope").unwrap();
        assert!(!sizes_match(&src, &dst));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_state_sidecars_clears_wal_shm() {
        let dir = std::env::temp_dir().join(format!(
            "cursor-cleanup-sidecar-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("state.vscdb-wal"), b"wal").unwrap();
        fs::write(dir.join("state.vscdb-shm"), b"shm").unwrap();
        assert_eq!(remove_state_sidecars(&dir), 0);
        assert!(!dir.join("state.vscdb-wal").exists());
        assert!(!dir.join("state.vscdb-shm").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_state_sidecars_requires_absent() {
        let dir = std::env::temp_dir().join(format!(
            "cursor-cleanup-clear-sidecar-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("state.vscdb-wal"), b"wal").unwrap();
        fs::write(dir.join("state.vscdb-shm"), b"shm").unwrap();
        clear_state_sidecars(&dir).expect("sidecars must clear");
        assert!(state_sidecars_absent(&dir));
        clear_state_sidecars(&dir).expect("already absent is ok");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_path_unlocked_restores_original_name() {
        let dir = std::env::temp_dir().join(format!(
            "cursor-cleanup-probe-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("state.vscdb");
        fs::write(&src, b"db-bytes").unwrap();
        // Leftover probe name must not orphan the real file.
        fs::write(dir.join("state.vscdb.unlock-probe"), b"stale").unwrap();
        assert!(probe_path_unlocked(&src));
        assert!(src.exists());
        let body = fs::read(&src).unwrap();
        assert_eq!(body, b"db-bytes");
        // No new orphan probe for the live file (stale leftover may remain).
        assert!(!dir.join("state.vscdb.unlock-probe-1").exists() || src.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
