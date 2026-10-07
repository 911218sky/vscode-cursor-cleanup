mod backup;
mod cleanup;
mod fsutil;
mod i18n;
mod paths;
mod ui;

use i18n::{t, Lang, Msg};
use std::env;
use std::io::{self, Write};
use ui::EditorOutcome;

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
            // Accepted as a no-op for old scripts; cleanup no longer prompts for backup.
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

fn main() {
    #[cfg(windows)]
    {
        let _ = colored::control::set_virtual_terminal(true);
    }

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

    if cli.yes && cli.app.is_none() {
        eprintln!("--yes requires --app cursor|vscode|both");
        std::process::exit(2);
    }

    let interactive = env::args().len() <= 1;
    let should_pause = !cli.no_pause && !cli.yes && (interactive || ui::is_gui_launch());

    if interactive && cli.lang.is_none() {
        ui::pick_language_interactive();
    }

    // Interactive main menu
    if !(cli.app.is_some() || cli.yes || cli.scan) {
        loop {
            ui::clear_screen();
            match ui::main_menu() {
                0 => {
                    // Clean
                    ui::clear_screen();
                    let mut finished = false;
                    loop {
                        let Some(editors) = ui::pick_editors(None) else {
                            break;
                        };
                        let mut back = false;
                        for editor in editors {
                            match ui::run_editor(editor, false) {
                                EditorOutcome::BackToApps => {
                                    back = true;
                                    break;
                                }
                                EditorOutcome::Done => {
                                    finished = true;
                                }
                            }
                        }
                        if back {
                            ui::clear_screen();
                            continue;
                        }
                        break;
                    }
                    if finished {
                        ui::pause_menu();
                    }
                }
                1 => {
                    ui::clear_screen();
                    if ui::run_backup_flow() {
                        ui::pause_menu();
                    }
                }
                2 => {
                    ui::clear_screen();
                    if ui::run_restore_flow() {
                        ui::pause_menu();
                    }
                }
                _ => {
                    ui::dim(t(Msg::Exited));
                    break;
                }
            }
        }

        let _ = io::stdout().flush();
        if should_pause {
            ui::pause_exit();
        }
        return;
    }

    ui::print_banner();

    // CLI shortcuts: scan / yes clean only
    let Some(editors) = ui::pick_editors(cli.app.as_deref()) else {
        return;
    };
    if cli.scan {
        for editor in editors {
            if let Some((root, total, stats)) = cleanup::collect_stats(editor) {
                if root.exists() {
                    ui::print_scan(editor, &root, total, &stats);
                } else {
                    ui::warn(&i18n::fmt_no_dir(
                        editor.display_name(),
                        &root.display().to_string(),
                    ));
                }
            }
        }
        if should_pause {
            ui::pause_exit();
        }
        return;
    }
    for editor in editors {
        let _ = ui::run_editor(editor, cli.yes);
    }
    println!();
    ui::ok(t(Msg::AllDone));
    if should_pause {
        ui::pause_exit();
    }
}
