mod backup;
mod cleanup;
mod cli;
mod cursor_version;
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
            "--uninstall-run" => cli.uninstall_run = true,
            "--no-pause" => cli.no_pause = true,
            "--no-backup" => {}
            // Accepted for backward compatibility; tool is Cursor-only.
            "-a" | "--app" => {
                let _ = args.next();
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
        let root_ok = root.exists()
            || (scope == DataScope::Cli && editor.data_dir().is_some_and(|p| p.exists()));
        if root_ok {
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

    let interactive = env::args().len() <= 1;
    let pause_on_exit = !cli.no_pause && interactive;

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
