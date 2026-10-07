mod backup;
mod cleanup;
mod cli;
mod fsutil;
mod i18n;
mod paths;
mod tui;

use i18n::Lang;
use std::env;
use std::io::{self, Write};

struct Cli {
    app: Option<String>,
    lang: Option<Lang>,
    yes: bool,
    scan: bool,
    no_pause: bool,
    help: bool,
    version: bool,
}

fn parse_cli() -> Cli {
    let mut cli = Cli {
        app: None,
        lang: None,
        yes: false,
        scan: false,
        no_pause: false,
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
            "--no-pause" => cli.no_pause = true,
            "--no-backup" => {}
            "-a" | "--app" => cli.app = args.next(),
            "-l" | "--lang" => {
                if let Some(v) = args.next() {
                    cli.lang = Lang::parse(&v);
                    if cli.lang.is_none() {
                        eprintln!("unknown --lang: {v} (use zh-TW|zh-CN|en)");
                        cli.help = true;
                    }
                }
            }
            other if other.starts_with("--app=") => {
                cli.app = Some(other.trim_start_matches("--app=").to_string());
            }
            other if other.starts_with("--lang=") => {
                let v = other.trim_start_matches("--lang=");
                cli.lang = Lang::parse(v);
                if cli.lang.is_none() {
                    eprintln!("unknown --lang: {v} (use zh-TW|zh-CN|en)");
                    cli.help = true;
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
    if outcome.did_work {
        cli::pause_menu();
    }
    // Skip the extra "Press Enter to close" when the user already chose Exit.
    if pause_on_exit && !outcome.user_quit {
        cli::pause_exit();
    }
}

fn run_cli_scan(app: &str, pause_on_exit: bool) {
    cli::print_banner();
    let Some(editors) = cli::parse_editors(app) else {
        return;
    };
    for editor in editors {
        if let Some((root, total, stats)) = cleanup::collect_stats(editor) {
            if root.exists() {
                cli::print_scan(editor, &root, total, &stats);
            } else {
                cli::warn(&i18n::fmt_no_dir(
                    editor.display_name(),
                    &root.display().to_string(),
                ));
            }
        }
    }
    if pause_on_exit {
        cli::pause_exit();
    }
}

fn run_cli_yes(app: &str, pause_on_exit: bool) {
    cli::print_banner();
    let Some(editors) = cli::parse_editors(app) else {
        return;
    };
    for editor in editors {
        cli::run_yes_clean(editor);
    }
    println!();
    cli::ok(i18n::t(i18n::Msg::AllDone));
    if pause_on_exit {
        cli::pause_exit();
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

    // TUI: double-click or bare `cursor-cleanup.exe`
    if interactive && !cli.yes && !cli.scan {
        run_tui(cli.lang.is_some(), pause_on_exit);
        return;
    }

    // CLI shortcuts require --app
    let Some(app) = cli.app.as_deref() else {
        if cli.yes || cli.scan {
            eprintln!("--scan and --yes require --app cursor|vscode|both");
            std::process::exit(2);
        }
        run_tui(cli.lang.is_some(), pause_on_exit);
        return;
    };

    if cli.yes && cli.scan {
        eprintln!("--scan and --yes cannot be used together");
        std::process::exit(2);
    }

    let pause = !cli.no_pause;
    if cli.scan {
        run_cli_scan(app, pause);
    } else if cli.yes {
        run_cli_yes(app, pause);
    } else {
        // e.g. --app cursor --lang en → still open TUI, pre-filter not wired; use TUI
        run_tui(cli.lang.is_some(), pause_on_exit);
    }
}
