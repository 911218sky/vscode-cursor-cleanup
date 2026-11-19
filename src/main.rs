mod backup;
mod cleanup;
mod cli;
mod cursor_settings;
mod cursor_version;
mod elevate;
mod fsutil;
mod i18n;
mod paths;
mod tui;
mod uninstall;

use i18n::Lang;
use paths::DataScope;
use std::env;
use std::io::{self, IsTerminal, Write};

struct Cli {
    lang: Option<Lang>,
    scope: DataScope,
    yes: bool,
    scan: bool,
    no_pause: bool,
    /// Elevated / non-TUI full uninstall pipeline.
    uninstall_run: bool,
    /// Dry-run uninstall preview (never deletes).
    uninstall_plan: bool,
    show_cursor_version: bool,
    set_version: Option<String>,
    restore_version: bool,
    /// Manual Cursor install path (folder or Cursor.exe).
    cursor_path: Option<String>,
    disable_updates: bool,
    show_update_settings: bool,
    help: bool,
    version: bool,
}

fn parse_cli() -> Cli {
    let mut cli = Cli {
        lang: None,
        scope: DataScope::Both,
        yes: false,
        scan: false,
        no_pause: false,
        uninstall_run: false,
        uninstall_plan: false,
        show_cursor_version: false,
        set_version: None,
        restore_version: false,
        cursor_path: None,
        disable_updates: false,
        show_update_settings: false,
        help: false,
        version: false,
    };
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => cli.help = true,
            "-V" | "--version" => cli.version = true,
            "-y" | "--yes" => cli.yes = true,
            "--scan" => cli.scan = true,
            "--show-version" => cli.show_cursor_version = true,
            "--restore-version" => cli.restore_version = true,
            "--disable-updates" => cli.disable_updates = true,
            "--show-update-settings" => cli.show_update_settings = true,
            "--uninstall-run" => cli.uninstall_run = true,
            "--uninstall-plan" => cli.uninstall_plan = true,
            "--no-pause" => cli.no_pause = true,
            "--no-backup" => {}
            // Accepted for backward compatibility; tool is Cursor-only.
            "-a" | "--app" => {
                let _ = args.next();
            }
            "--set-version" => {
                if let Some(v) = args.next() {
                    cli.set_version = Some(v);
                } else {
                    eprintln!("--set-version requires <x.y.z>");
                    cli.help = true;
                }
            }
            "--cursor-path" => {
                if let Some(v) = args.next() {
                    cli.cursor_path = Some(v);
                } else {
                    eprintln!("--cursor-path requires <path>");
                    cli.help = true;
                }
            }
            "-l" | "--lang" => {
                if let Some(v) = args.next() {
                    cli.lang = Lang::parse(&v);
                    if cli.lang.is_none() {
                        eprintln!("unknown --lang: {v} (use zh-TW|zh-CN|en)");
                        cli.help = true;
                    }
                }
            }
            "-s" | "--scope" => {
                if let Some(v) = args.next() {
                    match DataScope::parse(&v) {
                        Some(s) => cli.scope = s,
                        None => {
                            eprintln!("unknown --scope: {v} (use ide|cli|both)");
                            cli.help = true;
                        }
                    }
                }
            }
            other if other.starts_with("--app=") => {}
            other if other.starts_with("--set-version=") => {
                cli.set_version = Some(other.trim_start_matches("--set-version=").to_string());
            }
            other if other.starts_with("--cursor-path=") => {
                cli.cursor_path = Some(other.trim_start_matches("--cursor-path=").to_string());
            }
            other if other.starts_with("--lang=") => {
                let v = other.trim_start_matches("--lang=");
                cli.lang = Lang::parse(v);
                if cli.lang.is_none() {
                    eprintln!("unknown --lang: {v} (use zh-TW|zh-CN|en)");
                    cli.help = true;
                }
            }
            other if other.starts_with("--scope=") => {
                let v = other.trim_start_matches("--scope=");
                match DataScope::parse(v) {
                    Some(s) => cli.scope = s,
                    None => {
                        eprintln!("unknown --scope: {v} (use ide|cli|both)");
                        cli.help = true;
                    }
                }
            }
            other => {
                eprintln!("unknown option: {other}");
                cli.help = true;
            }
        }
    }
    cli
}

fn run_tui(skip_lang: bool, pause_on_exit: bool) {
    let outcome = match tui::run(skip_lang) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("TUI error: {e}");
            std::process::exit(1);
        }
    };
    let _ = io::stdout().flush();

    // Windows: leave TUI, then UAC-elevate a child that runs --uninstall-run.
    if outcome.elevate_uninstall {
        println!();
        println!("  {}", i18n::t(i18n::Msg::UninstallNeedAdmin));
        let _ = io::stdout().flush();
        if !uninstall::relaunch_elevated_uninstall(outcome.uninstall_scope) {
            cli::warn(i18n::t(i18n::Msg::UninstallElevateFail));
            if io::stdin().is_terminal() {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
        return;
    }

    // Leave alternate screen first, then run the official uninstaller + wipe.
    if outcome.run_uninstall {
        let ok = uninstall::run_uninstall_cli(outcome.uninstall_scope);
        if io::stdin().is_terminal() {
            cli::pause_exit();
        }
        if !ok {
            std::process::exit(1);
        }
        return;
    }

    // Exit / Esc / Ctrl+C already left the TUI (main menu is inside the TUI).
    if outcome.user_quit {
        return;
    }
    if pause_on_exit {
        cli::pause_exit();
    }
}

fn run_cli_scan(scope: DataScope, pause_on_exit: bool) {
    cli::print_banner();
    let editor = paths::Editor::Cursor;
    if let Some((root, total, stats)) = cleanup::collect_stats_for(editor, scope) {
        if cleanup::scope_data_available(editor, scope, &root) {
            cli::print_scan(editor, scope, &root, total, &stats);
        } else {
            cli::warn(&i18n::fmt_no_dir(
                editor.display_name(),
                &root.display().to_string(),
            ));
        }
    }
    if pause_on_exit {
        cli::pause_exit();
    }
}

fn run_cli_yes(scope: DataScope, pause_on_exit: bool) {
    cli::print_banner();
    let ok = cli::run_yes_clean(paths::Editor::Cursor, scope);
    println!();
    if ok {
        cli::ok(i18n::t(i18n::Msg::AllDone));
    }
    if pause_on_exit {
        cli::pause_exit();
    }
    if !ok {
        std::process::exit(1);
    }
}

fn run_cli_show_version(pause_on_exit: bool) {
    cli::print_banner();
    match cursor_version::read_installed_versions() {
        Some((p, k)) => {
            if let Some((prod, pkg)) = cursor_version::version_json_paths() {
                println!("  product.json  {}", prod.display());
                println!("  package.json  {}", pkg.display());
            }
            cli::ok(&i18n::fmt_version_current(&p, &k));
        }
        None => {
            cli::warn(i18n::t(i18n::Msg::VersionNotFound));
            println!("  {}", i18n::t(i18n::Msg::VersionPathHint));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
    }
    if pause_on_exit {
        cli::pause_exit();
    }
}

fn run_cli_set_version(ver: &str, pause_on_exit: bool) {
    cli::print_banner();
    match cursor_version::set_installed_version(ver) {
        Ok((old, new)) if old == new => {
            cli::ok(i18n::t(i18n::Msg::VersionSame));
        }
        Ok((old, new)) => {
            cli::ok(&i18n::fmt_version_changed(&old, &new));
            println!("  {}", i18n::t(i18n::Msg::VersionRestartHint));
        }
        Err(cursor_version::SetVersionError::InvalidVersion) => {
            cli::warn(i18n::t(i18n::Msg::VersionInvalid));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
        Err(cursor_version::SetVersionError::InstallNotFound) => {
            cli::warn(i18n::t(i18n::Msg::VersionNotFound));
            println!("  {}", i18n::t(i18n::Msg::VersionPathHint));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
        Err(e) if e.is_permission() => {
            if elevate::is_process_elevated() {
                cli::warn(i18n::t(i18n::Msg::VersionPermission));
                if pause_on_exit {
                    cli::pause_exit();
                }
                std::process::exit(1);
            }
            println!("  {}", i18n::t(i18n::Msg::VersionNeedAdmin));
            let _ = io::stdout().flush();
            if !elevate::elevate_set_version(ver) {
                cli::warn(i18n::t(i18n::Msg::VersionElevateCancel));
                if pause_on_exit {
                    cli::pause_exit();
                }
                std::process::exit(1);
            }
            // Re-read after elevated child finished.
            match cursor_version::read_installed_versions() {
                Some((p, k)) if p == ver || k == ver => {
                    cli::ok(i18n::t(i18n::Msg::VersionElevateOk));
                    cli::ok(&i18n::fmt_version_current(&p, &k));
                    println!("  {}", i18n::t(i18n::Msg::VersionRestartHint));
                }
                Some((p, k)) => {
                    cli::warn(&format!(
                        "{} — {}",
                        i18n::t(i18n::Msg::VersionFail),
                        i18n::fmt_version_current(&p, &k)
                    ));
                    if pause_on_exit {
                        cli::pause_exit();
                    }
                    std::process::exit(1);
                }
                None => {
                    cli::ok(i18n::t(i18n::Msg::VersionElevateOk));
                }
            }
        }
        Err(e) => {
            cli::warn(&format!("{}: {}", i18n::t(i18n::Msg::VersionFail), e.detail()));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
    }
    if pause_on_exit {
        cli::pause_exit();
    }
}

fn run_cli_show_update_settings(pause_on_exit: bool) {
    cli::print_banner();
    match cursor_settings::read_update_status() {
        Some(st) => {
            println!("  {}", st.path.display());
            cli::ok(&i18n::fmt_updates_status(
                st.mode.as_deref(),
                st.win_bg,
            ));
            if st.is_disabled() {
                println!("  {}", i18n::t(i18n::Msg::UpdatesAlreadyOff));
            }
        }
        None => {
            cli::warn(i18n::t(i18n::Msg::UpdatesSettingsMissing));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
    }
    if pause_on_exit {
        cli::pause_exit();
    }
}

fn run_cli_disable_updates(pause_on_exit: bool) {
    cli::print_banner();
    match cursor_settings::disable_updates() {
        Ok(r) if !r.changed => {
            cli::ok(i18n::t(i18n::Msg::UpdatesAlreadyOff));
            println!("  {}", r.path.display());
        }
        Ok(r) => {
            cli::ok(&i18n::fmt_updates_disabled(&r.path.display().to_string()));
            println!(
                "  {}",
                i18n::fmt_updates_status(Some(&r.mode), Some(r.win_bg))
            );
            println!("  {}", i18n::t(i18n::Msg::UpdatesRestartHint));
        }
        Err(cursor_settings::DisableUpdatesError::NoDataDir) => {
            cli::warn(i18n::t(i18n::Msg::UpdatesSettingsMissing));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
        Err(e) => {
            cli::warn(&format!("{}: {}", i18n::t(i18n::Msg::UpdatesFail), e.detail()));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
    }
    if pause_on_exit {
        cli::pause_exit();
    }
}

fn run_cli_restore_version(pause_on_exit: bool) {
    cli::print_banner();
    match cursor_version::restore_installed_version() {
        Ok(n) => {
            cli::ok(&i18n::fmt_version_restored(n));
            println!("  {}", i18n::t(i18n::Msg::VersionRestartHint));
        }
        Err(cursor_version::RestoreVersionError::InstallNotFound) => {
            cli::warn(i18n::t(i18n::Msg::VersionNotFound));
            println!("  {}", i18n::t(i18n::Msg::VersionPathHint));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
        Err(cursor_version::RestoreVersionError::NoBackup) => {
            cli::warn(i18n::t(i18n::Msg::VersionNoBackup));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
        Err(e) if e.is_permission() => {
            if elevate::is_process_elevated() {
                cli::warn(i18n::t(i18n::Msg::VersionPermission));
                if pause_on_exit {
                    cli::pause_exit();
                }
                std::process::exit(1);
            }
            println!("  {}", i18n::t(i18n::Msg::VersionNeedAdmin));
            let _ = io::stdout().flush();
            if !elevate::elevate_restore_version() {
                cli::warn(i18n::t(i18n::Msg::VersionElevateCancel));
                if pause_on_exit {
                    cli::pause_exit();
                }
                std::process::exit(1);
            }
            cli::ok(i18n::t(i18n::Msg::VersionElevateOk));
            if let Some((p, k)) = cursor_version::read_installed_versions() {
                cli::ok(&i18n::fmt_version_current(&p, &k));
            }
            println!("  {}", i18n::t(i18n::Msg::VersionRestartHint));
        }
        Err(e) => {
            cli::warn(&format!("{}: {}", i18n::t(i18n::Msg::VersionFail), e.detail()));
            if pause_on_exit {
                cli::pause_exit();
            }
            std::process::exit(1);
        }
    }
    if pause_on_exit {
        cli::pause_exit();
    }
}

fn apply_cursor_path_or_exit(raw: &str, pause_on_exit: bool) {
    if cursor_version::set_install_path_override(raw).is_err() {
        cli::warn(i18n::t(i18n::Msg::VersionPathInvalid));
        println!("  {}", i18n::t(i18n::Msg::VersionPathHint));
        if pause_on_exit {
            cli::pause_exit();
        }
        std::process::exit(1);
    }
}

fn main() {
    let cli = parse_cli();

    if let Some(lang) = cli.lang {
        i18n::set_lang(lang);
    }

    if cli.help {
        print!("{}", i18n::help_text());
        return;
    }
    if cli.version {
        println!("cursor-cleanup {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    // Apply before any version / relaunch path resolution (including elevated child).
    if let Some(ref path) = cli.cursor_path {
        apply_cursor_path_or_exit(path, !cli.no_pause);
    }

    let interactive = env::args().len() <= 1;
    let pause_on_exit = !cli.no_pause && interactive;

    if cli.uninstall_plan && cli.uninstall_run {
        eprintln!("--uninstall-plan and --uninstall-run cannot be used together");
        std::process::exit(2);
    }

    if cli.uninstall_plan {
        cli::print_banner();
        uninstall::print_uninstall_plan(cli.scope);
        if !cli.no_pause {
            cli::pause_exit();
        }
        return;
    }

    if cli.uninstall_run {
        let ok = uninstall::run_uninstall_cli(cli.scope);
        if !cli.no_pause {
            cli::pause_exit();
        }
        if !ok {
            std::process::exit(1);
        }
        return;
    }

    if cli.yes && cli.scan {
        eprintln!("--scan and --yes cannot be used together");
        std::process::exit(2);
    }

    let version_cmd = cli.show_cursor_version || cli.set_version.is_some() || cli.restore_version;
    let update_cmd = cli.disable_updates || cli.show_update_settings;
    if (version_cmd || update_cmd) && (cli.yes || cli.scan) {
        eprintln!("version/update flags cannot be combined with --scan / --yes");
        std::process::exit(2);
    }
    if [
        cli.show_cursor_version as u8,
        cli.set_version.is_some() as u8,
        cli.restore_version as u8,
        cli.disable_updates as u8,
        cli.show_update_settings as u8,
    ]
    .iter()
    .sum::<u8>()
        > 1
    {
        eprintln!(
            "use only one of --show-version / --set-version / --restore-version / --disable-updates / --show-update-settings"
        );
        std::process::exit(2);
    }

    if cli.show_cursor_version {
        run_cli_show_version(!cli.no_pause);
        return;
    }
    if let Some(ver) = cli.set_version.as_deref() {
        run_cli_set_version(ver, !cli.no_pause);
        return;
    }
    if cli.restore_version {
        run_cli_restore_version(!cli.no_pause);
        return;
    }
    if cli.show_update_settings {
        run_cli_show_update_settings(!cli.no_pause);
        return;
    }
    if cli.disable_updates {
        run_cli_disable_updates(!cli.no_pause);
        return;
    }

    // TUI: double-click or bare `cursor-cleanup.exe`
    if !cli.yes && !cli.scan {
        run_tui(cli.lang.is_some(), pause_on_exit);
        return;
    }

    let pause = !cli.no_pause;
    if cli.scan {
        run_cli_scan(cli.scope, pause);
    } else {
        run_cli_yes(cli.scope, pause);
    }
}
