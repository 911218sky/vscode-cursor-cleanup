use crate::cleanup::resolve_editor_exe;
use crate::paths::Editor;
use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Detect the installed Cursor app version from `product.json`.
pub fn detect_installed() -> Option<String> {
    let (prod, _) = version_json_paths()?;
    fs::read_to_string(prod)
        .ok()
        .and_then(|text| root_json_string_field(&text, "version"))
        .filter(|v| !v.is_empty())
}

/// Current top-level versions from product.json / package.json.
pub fn read_installed_versions() -> Option<(String, String)> {
    let (prod, pkg) = version_json_paths()?;
    let pv = fs::read_to_string(&prod)
        .ok()
        .and_then(|t| root_json_string_field(&t, "version"))?;
    let kv = fs::read_to_string(&pkg)
        .ok()
        .and_then(|t| root_json_string_field(&t, "version"))?;
    Some((pv, kv))
}

/// Paths to `product.json` and sibling `package.json`.
pub fn version_json_paths() -> Option<(PathBuf, PathBuf)> {
    let loc = resolve_editor_exe(Editor::Cursor)?;
    let prod = product_json_path(&loc)?;
    let pkg = prod.with_file_name("package.json");
    if !pkg.is_file() {
        return None;
    }
    Some((prod, pkg))
}

/// Strict `major.minor.patch` digits only (same rule as cursor-login spoof).
pub fn is_strict_semver(ver: &str) -> bool {
    let ver = ver.trim();
    let mut parts = ver.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(c), None) => {
            !a.is_empty()
                && !b.is_empty()
                && !c.is_empty()
                && a.chars().all(|ch| ch.is_ascii_digit())
                && b.chars().all(|ch| ch.is_ascii_digit())
                && c.chars().all(|ch| ch.is_ascii_digit())
        }
        _ => false,
    }
}

#[derive(Debug)]
pub enum SetVersionError {
    InstallNotFound,
    InvalidVersion,
    Read(PathBuf, String),
    Write(PathBuf, String),
    NoTopLevelVersion(PathBuf),
}

impl SetVersionError {
    pub fn detail(&self) -> String {
        match self {
            SetVersionError::InstallNotFound => "install not found".into(),
            SetVersionError::InvalidVersion => "invalid version".into(),
            SetVersionError::Read(p, msg) => format!("{}: {msg}", p.display()),
            SetVersionError::Write(p, msg) => format!("{}: {msg}", p.display()),
            SetVersionError::NoTopLevelVersion(p) => {
                format!("{}: no top-level version", p.display())
            }
        }
    }

    pub fn is_permission(&self) -> bool {
        matches!(self, SetVersionError::Write(_, msg) if {
            let lower = msg.to_ascii_lowercase();
            lower.contains("permission")
                || lower.contains("access is denied")
                || lower.contains("os error 5")
                || msg.contains("拒絕存取")
                || msg.contains("拒绝访问")
                || msg.contains("存取被拒")
                || msg.contains("访问被拒绝")
        })
    }
}

/// Rewrite only the root `"version"` in product.json + package.json.
/// Creates `.bak` once per file (never overwrites an existing bak).
/// Returns `(old_product_version, new_version)`.
pub fn set_installed_version(ver: &str) -> Result<(String, String), SetVersionError> {
    let ver = ver.trim();
    if !is_strict_semver(ver) {
        return Err(SetVersionError::InvalidVersion);
    }
    let (prod, pkg) = version_json_paths().ok_or(SetVersionError::InstallNotFound)?;

    let old = read_json_version(&prod)?;
    let pkg_old = read_json_version(&pkg)?;

    if old == ver && pkg_old == ver {
        return Ok((old, ver.to_string()));
    }

    if old != ver {
        backup_once(&prod).map_err(|e| map_write_err(&prod, e))?;
        write_json_version(&prod, ver)?;
    }
    if pkg_old != ver {
        backup_once(&pkg).map_err(|e| map_write_err(&pkg, e))?;
        write_json_version(&pkg, ver)?;
    }
    Ok((old, ver.to_string()))
}

#[derive(Debug)]
pub enum RestoreVersionError {
    InstallNotFound,
    NoBackup,
    Write(PathBuf, String),
}

impl RestoreVersionError {
    pub fn detail(&self) -> String {
        match self {
            RestoreVersionError::InstallNotFound => "install not found".into(),
            RestoreVersionError::NoBackup => "no backup".into(),
            RestoreVersionError::Write(p, msg) => format!("{}: {msg}", p.display()),
        }
    }

    pub fn is_permission(&self) -> bool {
        matches!(self, RestoreVersionError::Write(_, msg) if {
            let lower = msg.to_ascii_lowercase();
            lower.contains("permission")
                || lower.contains("access is denied")
                || lower.contains("os error 5")
                || msg.contains("拒絕存取")
                || msg.contains("拒绝访问")
                || msg.contains("存取被拒")
                || msg.contains("访问被拒绝")
        })
    }
}

/// Restore product.json / package.json from sibling `.bak` files.
/// Returns how many files were restored.
pub fn restore_installed_version() -> Result<usize, RestoreVersionError> {
    let (prod, pkg) = version_json_paths().ok_or(RestoreVersionError::InstallNotFound)?;
    let mut n = 0usize;
    for p in [prod, pkg] {
        let bak = backup_path(&p);
        let Ok(bytes) = fs::read(&bak) else {
            continue;
        };
        fs::write(&p, bytes).map_err(|e| RestoreVersionError::Write(p.clone(), e.to_string()))?;
        n += 1;
    }
    if n == 0 {
        return Err(RestoreVersionError::NoBackup);
    }
    Ok(n)
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

fn backup_path(path: &Path) -> PathBuf {
    // product.json → product.json.bak (same as cursor-login)
    let mut s = path.as_os_str().to_os_string();
    s.push(".bak");
    PathBuf::from(s)
}

fn backup_once(path: &Path) -> io::Result<()> {
    let bak = backup_path(path);
    if bak.is_file() {
        return Ok(());
    }
    fs::copy(path, bak)?;
    Ok(())
}

fn map_write_err(path: &Path, err: io::Error) -> SetVersionError {
    SetVersionError::Write(path.to_path_buf(), err.to_string())
}

fn read_json_version(path: &Path) -> Result<String, SetVersionError> {
    let raw = fs::read_to_string(path)
        .map_err(|e| SetVersionError::Read(path.to_path_buf(), e.to_string()))?;
    root_json_string_field(&raw, "version")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| SetVersionError::NoTopLevelVersion(path.to_path_buf()))
}

fn write_json_version(path: &Path, ver: &str) -> Result<(), SetVersionError> {
    let raw = fs::read(path).map_err(|e| SetVersionError::Read(path.to_path_buf(), e.to_string()))?;
    let out = replace_top_level_version(&raw, ver).map_err(|e| match e {
        ReplaceError::BadVersion => SetVersionError::InvalidVersion,
        ReplaceError::NoTopLevel | ReplaceError::BadJson => {
            SetVersionError::NoTopLevelVersion(path.to_path_buf())
        }
    })?;
    if out == raw {
        return Ok(());
    }
    fs::write(path, out).map_err(|e| map_write_err(path, e))
}

#[derive(Debug)]
enum ReplaceError {
    BadVersion,
    BadJson,
    NoTopLevel,
}

/// Rewrite only the root object's `"version"` string; every other byte stays.
fn replace_top_level_version(raw: &[u8], ver: &str) -> Result<Vec<u8>, ReplaceError> {
    if !is_strict_semver(ver) {
        return Err(ReplaceError::BadVersion);
    }
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < raw.len() {
        if raw[i] != b'"' {
            match raw[i] {
                b'{' | b'[' => depth += 1,
                b'}' | b']' => depth -= 1,
                _ => {}
            }
            i += 1;
            continue;
        }
        let (key, end) = read_json_string(raw, i).ok_or(ReplaceError::BadJson)?;
        if depth != 1 || key != "version" {
            i = end + 1;
            continue;
        }
        let mut j = end + 1;
        while j < raw.len() && is_json_space(raw[j]) {
            j += 1;
        }
        if j >= raw.len() || raw[j] != b':' {
            return Err(ReplaceError::BadJson);
        }
        j += 1;
        while j < raw.len() && is_json_space(raw[j]) {
            j += 1;
        }
        if j >= raw.len() || raw[j] != b'"' {
            return Err(ReplaceError::BadJson);
        }
        let (val, val_end) = read_json_string(raw, j).ok_or(ReplaceError::BadJson)?;
        if val == ver {
            return Ok(raw.to_vec());
        }
        let mut out = Vec::with_capacity(raw.len() + ver.len().saturating_sub(val.len()));
        out.extend_from_slice(&raw[..j + 1]);
        out.extend_from_slice(ver.as_bytes());
        out.extend_from_slice(&raw[val_end..]);
        return Ok(out);
    }
    Err(ReplaceError::NoTopLevel)
}

fn is_json_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// Read a JSON string starting at the opening quote.
/// Returns (unescaped text for simple escapes, index of closing quote).
fn read_json_string(raw: &[u8], i: usize) -> Option<(String, usize)> {
    if i >= raw.len() || raw[i] != b'"' {
        return None;
    }
    let mut b = String::new();
    let mut j = i + 1;
    while j < raw.len() {
        match raw[j] {
            b'\\' => {
                if j + 1 >= raw.len() {
                    return None;
                }
                b.push(raw[j + 1] as char);
                j += 2;
            }
            b'"' => return Some((b, j)),
            c => {
                b.push(c as char);
                j += 1;
            }
        }
    }
    None
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
    fn strict_semver_digits_only() {
        assert!(is_strict_semver("3.27.22"));
        assert!(!is_strict_semver("3.27"));
        assert!(!is_strict_semver("3.27.22-nightly"));
        assert!(!is_strict_semver("not-a-version"));
        assert!(!is_strict_semver(""));
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
    fn replace_top_level_version_only() {
        let raw = br#"{"name":"Cursor","nested":{"version":"1.2.3"},"version":"3.17.21","commit":"abc"}"#;
        let out = replace_top_level_version(raw, "3.27.22").unwrap();
        assert_eq!(
            std::str::from_utf8(&out).unwrap(),
            r#"{"name":"Cursor","nested":{"version":"1.2.3"},"version":"3.27.22","commit":"abc"}"#
        );

        let spaced = b"{\n  \"version\": \"3.17.21\",\n  \"name\": \"Cursor\"\n}\n";
        let out = replace_top_level_version(spaced, "3.27.22").unwrap();
        assert_eq!(
            std::str::from_utf8(&out).unwrap(),
            "{\n  \"version\": \"3.27.22\",\n  \"name\": \"Cursor\"\n}\n"
        );

        let same = replace_top_level_version(&out, "3.27.22").unwrap();
        assert_eq!(same, out);

        let escaped = br#"{"quote":"say \"hi\"","version":"3.17.21"}"#;
        let out = replace_top_level_version(escaped, "3.27.22").unwrap();
        assert_eq!(
            std::str::from_utf8(&out).unwrap(),
            r#"{"quote":"say \"hi\"","version":"3.27.22"}"#
        );
    }

    #[test]
    fn set_version_on_temp_install_copy() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("cursor-setver-{stamp}"));
        let app = root.join("resources").join("app");
        fs::create_dir_all(&app).unwrap();
        let prod = app.join("product.json");
        let pkg = app.join("package.json");
        let before = br#"{"name":"Cursor","nested":{"version":"1.2.3"},"version":"3.17.21","commit":"abc"}"#;
        fs::write(&prod, before).unwrap();
        fs::write(&pkg, "{\n  \"name\": \"cursor\",\n  \"version\": \"3.17.21\"\n}\n").unwrap();

        // Point resolve via a fake exe next to resources/
        let exe = root.join("Cursor.exe");
        fs::write(&exe, b"").unwrap();

        // Exercise path helpers directly (resolve_editor_exe may find a real install).
        assert_eq!(
            product_json_path(&exe).as_deref(),
            Some(prod.as_path())
        );

        let out = replace_top_level_version(&fs::read(&prod).unwrap(), "3.27.22").unwrap();
        fs::write(&prod, &out).unwrap();
        let bak = backup_path(&prod);
        fs::copy(&prod, &bak).unwrap(); // ensure bak helper naming
        // Restore original into bak for restore test: rewrite bak to original
        fs::write(&bak, before).unwrap();
        fs::write(&prod, &out).unwrap();

        let text = fs::read_to_string(&prod).unwrap();
        assert_eq!(root_json_string_field(&text, "version").as_deref(), Some("3.27.22"));
        assert!(text.contains(r#""nested":{"version":"1.2.3"}"#));
        assert!(text.contains(r#""commit":"abc""#));

        // restore from bak
        let restored = fs::read(&bak).unwrap();
        fs::write(&prod, restored).unwrap();
        let text = fs::read_to_string(&prod).unwrap();
        assert_eq!(root_json_string_field(&text, "version").as_deref(), Some("3.17.21"));

        let _ = fs::remove_dir_all(&root);
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
