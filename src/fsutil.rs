use std::fs;
use std::io;
use std::path::Path;

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
    if fs::remove_dir_all(path).is_err() {
        if path.exists() {
            errors += 1;
        }
    }
    (freed, errors)
}

pub fn copy_best_effort(src: &Path, dst: &Path) -> io::Result<u64> {
    if !src.exists() {
        return Ok(0);
    }
    if src.is_file() {
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
        total += copy_best_effort(&entry.path(), &dst.join(name))?;
    }
    Ok(total)
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
}
