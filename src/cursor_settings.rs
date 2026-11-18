//! Cursor User `settings.json` helpers — disable built-in updater.
//!
//! Path (Windows): `%APPDATA%\Cursor\User\settings.json`
//! Keys (same as Settings → Application → Update):
//!   `"update.mode": "none"`
//!   `"update.enableWindowsBackgroundUpdates": false`

use crate::paths::Editor;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const UPDATE_MODE_KEY: &str = "update.mode";
const UPDATE_MODE_VALUE: &str = "none";
const WIN_BG_KEY: &str = "update.enableWindowsBackgroundUpdates";

/// `%APPDATA%/Cursor/User/settings.json` (or macOS/Linux equivalents).
pub fn user_settings_path() -> Option<PathBuf> {
    let root = Editor::Cursor.data_dir()?;
    Some(root.join("User").join("settings.json"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatus {
    pub path: PathBuf,
    pub mode: Option<String>,
    pub win_bg: Option<bool>,
}

impl UpdateStatus {
    pub fn is_disabled(&self) -> bool {
        self.mode.as_deref() == Some(UPDATE_MODE_VALUE) && self.win_bg != Some(true)
    }
}

pub fn read_update_status() -> Option<UpdateStatus> {
    let path = user_settings_path()?;
    if !path.is_file() {
        return Some(UpdateStatus {
            path,
            mode: None,
            win_bg: None,
        });
    }
    let text = fs::read_to_string(&path).ok()?;
    Some(UpdateStatus {
        path,
        mode: root_json_string_field(&text, UPDATE_MODE_KEY),
        win_bg: root_json_bool_field(&text, WIN_BG_KEY),
    })
}

#[derive(Debug)]
pub enum DisableUpdatesError {
    NoDataDir,
    Read(String),
    Write(String),
    BadJson(String),
}

impl DisableUpdatesError {
    pub fn detail(&self) -> String {
        match self {
            DisableUpdatesError::NoDataDir => "Cursor data dir not found".into(),
            DisableUpdatesError::Read(m)
            | DisableUpdatesError::Write(m)
            | DisableUpdatesError::BadJson(m) => m.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisableUpdatesResult {
    pub path: PathBuf,
    pub changed: bool,
    pub mode: String,
    pub win_bg: bool,
}

/// Set `update.mode=none` and `update.enableWindowsBackgroundUpdates=false`.
/// Writes `settings.json.bak` once (never overwrites an existing bak).
pub fn disable_updates() -> Result<DisableUpdatesResult, DisableUpdatesError> {
    let path = user_settings_path().ok_or(DisableUpdatesError::NoDataDir)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| DisableUpdatesError::Write(e.to_string()))?;
    }

    let raw = if path.is_file() {
        fs::read_to_string(&path).map_err(|e| DisableUpdatesError::Read(e.to_string()))?
    } else {
        "{\n}\n".to_string()
    };

    let mut out = raw.clone();
    out = set_top_level_string(&out, UPDATE_MODE_KEY, UPDATE_MODE_VALUE)
        .map_err(DisableUpdatesError::BadJson)?;
    out = set_top_level_bool(&out, WIN_BG_KEY, false).map_err(DisableUpdatesError::BadJson)?;

    if out == raw {
        return Ok(DisableUpdatesResult {
            path,
            changed: false,
            mode: UPDATE_MODE_VALUE.to_string(),
            win_bg: false,
        });
    }

    backup_once(&path).map_err(|e| DisableUpdatesError::Write(e.to_string()))?;
    fs::write(&path, out.as_bytes()).map_err(|e| DisableUpdatesError::Write(e.to_string()))?;

    Ok(DisableUpdatesResult {
        path,
        changed: true,
        mode: UPDATE_MODE_VALUE.to_string(),
        win_bg: false,
    })
}

fn backup_once(path: &Path) -> io::Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let mut bak = path.as_os_str().to_os_string();
    bak.push(".bak");
    let bak = PathBuf::from(bak);
    if bak.is_file() {
        return Ok(());
    }
    fs::copy(path, bak)?;
    Ok(())
}

fn is_json_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

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

/// Brace-depth scan for a root string field (supports dotted keys like `update.mode`).
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
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

pub fn root_json_bool_field(text: &str, key: &str) -> Option<bool> {
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
                    return bool_value_after_colon(&text[after_key..]);
                }
                in_str = true;
                i += 1;
            }
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
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

fn bool_value_after_colon(rest: &str) -> Option<bool> {
    let mut s = rest.trim_start();
    if !s.starts_with(':') {
        return None;
    }
    s = s[1..].trim_start();
    if s.starts_with("true") {
        let tail = &s[4..];
        if tail.is_empty()
            || tail.starts_with(|c: char| c == ',' || c == '}' || c == ']' || c.is_whitespace())
        {
            return Some(true);
        }
    }
    if s.starts_with("false") {
        let tail = &s[5..];
        if tail.is_empty()
            || tail.starts_with(|c: char| c == ',' || c == '}' || c == ']' || c.is_whitespace())
        {
            return Some(false);
        }
    }
    None
}

fn set_top_level_string(text: &str, key: &str, value: &str) -> Result<String, String> {
    if let Some((val_start, val_end, cur)) = find_top_level_string_span(text.as_bytes(), key)? {
        if cur == value {
            return Ok(text.to_string());
        }
        let mut out = String::with_capacity(text.len() + value.len());
        out.push_str(&text[..val_start]);
        out.push_str(value);
        out.push_str(&text[val_end..]);
        return Ok(out);
    }
    insert_top_level_property(text, key, &format!("\"{value}\""))
}

fn set_top_level_bool(text: &str, key: &str, value: bool) -> Result<String, String> {
    let lit = if value { "true" } else { "false" };
    if let Some((val_start, val_end, cur)) = find_top_level_bool_span(text.as_bytes(), key)? {
        if cur == value {
            return Ok(text.to_string());
        }
        let mut out = String::with_capacity(text.len() + 5);
        out.push_str(&text[..val_start]);
        out.push_str(lit);
        out.push_str(&text[val_end..]);
        return Ok(out);
    }
    insert_top_level_property(text, key, lit)
}

/// Returns (value_start, value_end_exclusive, current) for a root string property.
fn find_top_level_string_span(
    raw: &[u8],
    key: &str,
) -> Result<Option<(usize, usize, String)>, String> {
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
        let (k, end) = read_json_string(raw, i).ok_or_else(|| "bad json string".to_string())?;
        if depth != 1 || k != key {
            i = end + 1;
            continue;
        }
        let mut j = end + 1;
        while j < raw.len() && is_json_space(raw[j]) {
            j += 1;
        }
        if j >= raw.len() || raw[j] != b':' {
            return Err("key without colon".into());
        }
        j += 1;
        while j < raw.len() && is_json_space(raw[j]) {
            j += 1;
        }
        if j >= raw.len() || raw[j] != b'"' {
            return Err("string value expected".into());
        }
        let (val, val_end) =
            read_json_string(raw, j).ok_or_else(|| "bad string value".to_string())?;
        // value bytes are between opening quote+1 and closing quote
        return Ok(Some((j + 1, val_end, val)));
    }
    Ok(None)
}

fn find_top_level_bool_span(
    raw: &[u8],
    key: &str,
) -> Result<Option<(usize, usize, bool)>, String> {
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
        let (k, end) = read_json_string(raw, i).ok_or_else(|| "bad json string".to_string())?;
        if depth != 1 || k != key {
            i = end + 1;
            continue;
        }
        let mut j = end + 1;
        while j < raw.len() && is_json_space(raw[j]) {
            j += 1;
        }
        if j >= raw.len() || raw[j] != b':' {
            return Err("key without colon".into());
        }
        j += 1;
        while j < raw.len() && is_json_space(raw[j]) {
            j += 1;
        }
        if raw[j..].starts_with(b"true") {
            return Ok(Some((j, j + 4, true)));
        }
        if raw[j..].starts_with(b"false") {
            return Ok(Some((j, j + 5, false)));
        }
        return Err("bool value expected".into());
    }
    Ok(None)
}

fn detect_indent(text: &str) -> &'static str {
    for line in text.lines() {
        if line.starts_with("    \"") {
            return "    ";
        }
        if line.starts_with("  \"") {
            return "  ";
        }
        if line.starts_with("\t\"") {
            return "\t";
        }
    }
    "    "
}

fn insert_top_level_property(text: &str, key: &str, value_literal: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    // Find the last top-level closing `}`
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    let mut last_close = None;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' | b'[' => depth += 1,
            b'}' => {
                if depth == 1 {
                    last_close = Some(i);
                }
                depth = depth.saturating_sub(1);
            }
            b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let close = last_close.ok_or_else(|| "no top-level object".to_string())?;
    let indent = detect_indent(text);

    // Content before the root closing `}` (trim trailing whitespace/newlines).
    let before = text[..close].trim_end();
    if before.ends_with('{') {
        let mut out = before.to_string();
        out.push('\n');
        out.push_str(indent);
        out.push('"');
        out.push_str(key);
        out.push_str("\": ");
        out.push_str(value_literal);
        out.push('\n');
        out.push('}');
        out.push_str(&text[close + 1..]);
        return Ok(out);
    }

    // Ensure the previous property ends with a comma, then append the new line.
    let mut out = before.to_string();
    if !out.trim_end().ends_with(',') {
        out.push(',');
    }
    out.push('\n');
    out.push_str(indent);
    out.push('"');
    out.push_str(key);
    out.push_str("\": ");
    out.push_str(value_literal);
    out.push('\n');
    out.push('}');
    out.push_str(&text[close + 1..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_existing_mode() {
        let json = r#"{
    "window.autoDetectColorScheme": true,
    "update.mode": "default",
    "remote.SSH.remotePlatform": {
        "sky-laptop-linux": "linux"
    }
}"#;
        assert_eq!(
            root_json_string_field(json, "update.mode").as_deref(),
            Some("default")
        );
        assert!(root_json_bool_field(json, WIN_BG_KEY).is_none());
    }

    #[test]
    fn set_mode_and_insert_win_bg() {
        let json = r#"{
    "window.autoDetectColorScheme": true,
    "update.mode": "default",
    "remote.SSH.remotePlatform": {
        "sky-laptop-linux": "linux"
    }
}"#;
        let out = set_top_level_string(json, UPDATE_MODE_KEY, "none").unwrap();
        assert_eq!(
            root_json_string_field(&out, UPDATE_MODE_KEY).as_deref(),
            Some("none")
        );
        assert!(out.contains(r#""sky-laptop-linux": "linux""#));

        let out = set_top_level_bool(&out, WIN_BG_KEY, false).unwrap();
        assert_eq!(root_json_bool_field(&out, WIN_BG_KEY), Some(false));
        // nested untouched
        assert!(out.contains(r#""sky-laptop-linux": "linux""#));
    }

    #[test]
    fn insert_into_empty_object() {
        let out = set_top_level_string("{\n}\n", UPDATE_MODE_KEY, "none").unwrap();
        assert_eq!(
            root_json_string_field(&out, UPDATE_MODE_KEY).as_deref(),
            Some("none")
        );
        let out = set_top_level_bool(&out, WIN_BG_KEY, false).unwrap();
        assert_eq!(root_json_bool_field(&out, WIN_BG_KEY), Some(false));
    }

    #[test]
    fn already_none_is_noop_for_mode() {
        let json = "{\n    \"update.mode\": \"none\"\n}\n";
        let out = set_top_level_string(json, UPDATE_MODE_KEY, "none").unwrap();
        assert_eq!(out, json);
    }

    #[test]
    fn status_disabled_when_mode_none_and_bg_not_true() {
        let st = UpdateStatus {
            path: PathBuf::from("x"),
            mode: Some("none".into()),
            win_bg: None,
        };
        assert!(st.is_disabled());
        let st2 = UpdateStatus {
            path: PathBuf::from("x"),
            mode: Some("none".into()),
            win_bg: Some(true),
        };
        assert!(!st2.is_disabled());
    }
}
