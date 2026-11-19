//! Windows UAC elevation helper (Start-Process -Verb RunAs).

use crate::cleanup;
use crate::i18n;
use std::process::Command;

/// Whether this process is running elevated (Administrator) on Windows.
pub fn is_process_elevated() -> bool {
    #[cfg(windows)]
    {
        Command::new("net")
            .arg("session")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
pub(crate) fn powershell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Relaunch this executable elevated with the given CLI args (Windows UAC).
/// Always appends `--no-pause` and `--lang <current>` when not already present.
/// Also forwards `--cursor-path` when a session override is set.
/// Returns true if the elevated process was started and exited successfully.
pub fn relaunch_elevated(extra_args: &[&str]) -> bool {
    #[cfg(windows)]
    {
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let lang = i18n::lang().code();
        let mut args: Vec<String> = extra_args.iter().map(|s| (*s).to_string()).collect();
        if !args.iter().any(|a| a == "--no-pause") {
            args.push("--no-pause".into());
        }
        if !args.iter().any(|a| a == "--lang" || a.starts_with("--lang=")) {
            args.push("--lang".into());
            args.push(lang.to_string());
        }
        let has_cursor_path = args
            .iter()
            .any(|a| a == "--cursor-path" || a.starts_with("--cursor-path="));
        if !has_cursor_path {
            if let Some(p) = cleanup::cursor_path_override() {
                args.push("--cursor-path".into());
                args.push(p.display().to_string());
            }
        }

        let file = powershell_quote(&exe.display().to_string());
        let arg_list = args
            .iter()
            .map(|a| powershell_quote(a))
            .collect::<Vec<_>>()
            .join(",");
        // PassThru + exit $p.ExitCode so UAC cancel / child failure is visible to the parent.
        let cmd = format!(
            "$p = Start-Process -FilePath {file} -ArgumentList {arg_list} -Verb RunAs -Wait -PassThru; if ($null -eq $p) {{ exit 1 }}; exit $p.ExitCode"
        );
        Command::new("powershell")
            .args(["-NoProfile", "-Command", &cmd])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let _ = extra_args;
        false
    }
}

/// Elevate to set Cursor install version (`--set-version`).
pub fn elevate_set_version(ver: &str) -> bool {
    relaunch_elevated(&["--set-version", ver])
}

/// Elevate to restore Cursor install version (`--restore-version`).
pub fn elevate_restore_version() -> bool {
    relaunch_elevated(&["--restore-version"])
}
