use crate::cleanup::resolve_editor_exe;
use crate::paths::Editor;
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

/// Detect the installed Cursor app version from `product.json`.
pub fn detect_installed() -> Option<String> {
    let loc = resolve_editor_exe(Editor::Cursor)?;
    product_json_path(&loc)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|text| root_json_string_field(&text, "version"))
        .filter(|v| !v.is_empty())
}

/// True when `current` is strictly older than `backup` (semver major.minor.patch).
/// Missing / unparseable versions → do not warn.
pub fn current_is_older_than_backup(current: &str, backup: &str) -> bool {
    match (parse_semver(current), parse_semver(backup)) {
        (Some(c), Some(b)) => c.cmp(&b) == Ordering::Less,
        _ => false,
    }
}

/// Warn only when both sides parse and the install is older than the backup stamp.
pub fn installed_older_than_backup(current: Option<&str>, backup: Option<&str>) -> bool {
    match (current, backup) {
        (Some(c), Some(b)) => current_is_older_than_backup(c, b),
        _ => false,
    }
}

pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let core = raw
        .trim()
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .trim();
    if core.is_empty() {
        return None;
    }
    let mut parts = core.split('.');
    let major = parse_num_prefix(parts.next()?)?;
    let minor = parts.next().map(parse_num_prefix).unwrap_or(Some(0))?;
    let patch = parts.next().map(parse_num_prefix).unwrap_or(Some(0))?;
    Some((major, minor, patch))
}

fn parse_num_prefix(s: &str) -> Option<u64> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Brace-depth scan: only keys at JSON object depth 1 (root).
pub fn root_json_string_field(text: &str, key: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    let needle = format!("\"{key}\"");

    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                if depth == 1 && text[i..].starts_with(&needle) {
                    let after_key = i + needle.len();
                    if let Some(val) = string_value_after_colon(&text[after_key..]) {
                        return Some(val);
                    }
                }
                in_str = true;
                i += 1;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'[' => {
                // Treat arrays as increasing depth so nested objects are > 1.
                depth += 1;
                i += 1;
            }
            b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn string_value_after_colon(rest: &str) -> Option<String> {
    let mut s = rest.trim_start();
    if !s.starts_with(':') {
        return None;
    }
    s = s[1..].trim_start();
    if !s.starts_with('"') {
        return None;
    }
    s = &s[1..];
    let mut out = String::new();
    let mut escaped = false;
    for c in s.chars() {
        if escaped {
            out.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            return Some(out);
        }
        out.push(c);
    }
    None
}

fn product_json_path(exe_or_app: &Path) -> Option<PathBuf> {
    let p = exe_or_app;
    let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if name.ends_with(".app") {
        let cand = p.join("Contents/Resources/app/product.json");
        if cand.is_file() {
            return Some(cand);
        }
    }

    // Walk up a few parents looking for resources/app/product.json
    let mut dir = if p.is_file() {
        p.parent().map(|d| d.to_path_buf())
    } else {
        Some(p.to_path_buf())
    };
    for _ in 0..6 {
        let Some(d) = dir else {
            break;
        };
        let cand = d.join("resources").join("app").join("product.json");
        if cand.is_file() {
            return Some(cand);
        }
        let mac = d.join("Contents/Resources/app/product.json");
        if mac.is_file() {
            return Some(mac);
        }
        dir = d.parent().map(|x| x.to_path_buf());
    }

    for extra in [
        PathBuf::from("/usr/share/cursor/resources/app/product.json"),
        PathBuf::from("/usr/lib/cursor/resources/app/product.json"),
        PathBuf::from("/opt/cursor/resources/app/product.json"),
    ] {
        if extra.is_file() {
            return Some(extra);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_semver_core_and_suffix() {
        assert_eq!(parse_semver("1.2.9"), Some((1, 2, 9)));
        assert_eq!(parse_semver("1.2.10"), Some((1, 2, 10)));
        assert_eq!(parse_semver(" 2.1.36-nightly "), Some((2, 1, 36)));
        assert_eq!(parse_semver("0.50"), Some((0, 50, 0)));
        assert!(parse_semver("").is_none());
        assert!(parse_semver("abc").is_none());
    }

    #[test]
    fn cmp_1_2_9_older_than_1_2_10() {
        assert!(current_is_older_than_backup("1.2.9", "1.2.10"));
        assert!(!current_is_older_than_backup("1.2.10", "1.2.9"));
        assert!(!current_is_older_than_backup("1.2.10", "1.2.10"));
        assert!(!current_is_older_than_backup("2.0.0", "1.99.99"));
    }

    #[test]
    fn missing_or_unparseable_does_not_warn() {
        assert!(!current_is_older_than_backup("", "1.0.0"));
        assert!(!current_is_older_than_backup("1.0.0", ""));
        assert!(!current_is_older_than_backup("nope", "1.0.0"));
        assert!(!installed_older_than_backup(None, Some("1.0.0")));
        assert!(!installed_older_than_backup(Some("0.1.0"), None));
        assert!(installed_older_than_backup(Some("1.2.9"), Some("1.2.10")));
    }

    #[test]
    fn root_version_skips_nested_objects() {
        let json = r#"{
  "builtInExtensions": [
    { "name": "foo", "version": "9.9.9" }
  ],
  "version": "2.1.36",
  "nested": { "version": "0.0.1" }
}"#;
        assert_eq!(
            root_json_string_field(json, "version").as_deref(),
            Some("2.1.36")
        );
    }

    #[test]
    fn root_version_missing() {
        let json = r#"{ "nameShort": "Cursor", "nested": { "version": "1.0.0" } }"#;
        assert!(root_json_string_field(json, "version").is_none());
    }

    #[test]
    fn product_json_next_to_exe() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("cursor-ver-{stamp}"));
        let app = root.join("resources").join("app");
        fs::create_dir_all(&app).unwrap();
        fs::write(
            app.join("product.json"),
            r#"{ "version": "3.4.5", "builtInExtensions": [{ "version": "0.0.1" }] }"#,
        )
        .unwrap();
        let exe = root.join("Cursor.exe");
        fs::write(&exe, b"").unwrap();
        let found = product_json_path(&exe).unwrap();
        let text = fs::read_to_string(&found).unwrap();
        assert_eq!(
            root_json_string_field(&text, "version").as_deref(),
            Some("3.4.5")
        );
        let _ = fs::remove_dir_all(&root);
    }
}
