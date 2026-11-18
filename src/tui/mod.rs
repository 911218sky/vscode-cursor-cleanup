mod app;
mod terminal;
mod theme;
mod widgets;

use app::{App, ElevateRequest};
use crate::elevate;
use crate::i18n::{t, Msg};
use crate::paths::DataScope;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout, Write};
use std::time::Duration;
use terminal::TerminalGuard;

/// Result of an interactive TUI session.
pub struct TuiOutcome {
    /// User chose Exit / Esc from root screens, or Ctrl+C.
    pub user_quit: bool,
    /// Leave TUI, then run the uninstall pipeline on the main console.
    pub run_uninstall: bool,
    /// Leave TUI, then UAC-relaunch with `--uninstall-run`.
    pub elevate_uninstall: bool,
    /// IDE / CLI / both — used with `run_uninstall` / `elevate_uninstall`.
    pub uninstall_scope: DataScope,
}

/// Run the full interactive TUI.
pub fn run(skip_lang: bool) -> io::Result<TuiOutcome> {
    let mut guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut app = App::new(skip_lang);

    loop {
        terminal.draw(|f| app.draw(f))?;

        if app.should_quit {
            break;
        }

        if app.wants_animation() {
            // Spinner / progress screens: ~10 Hz tick + redraw.
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers,
                        ..
                    }) if modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    ev => app.handle_event(ev),
                }
            }
            app.tick();
        } else {
            // Menus: block until input — no idle wake or redraw (CPU ~0).
            match event::read()? {
                Event::Key(KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers,
                    ..
                }) if modifiers.contains(KeyModifiers::CONTROL) => {
                    app.should_quit = true;
                }
                ev => app.handle_event(ev),
            }
        }

        // Version write to Program Files: suspend TUI → UAC → resume (no EXE restart).
        if let Some(req) = app.take_elevate_request() {
            let ok = guard.suspend(|| {
                println!();
                println!("  {}", t(Msg::VersionNeedAdmin));
                let _ = io::stdout().flush();
                let elevated = match &req {
                    ElevateRequest::SetVersion(ver) => elevate::elevate_set_version(ver),
                    ElevateRequest::RestoreVersion => elevate::elevate_restore_version(),
                };
                if !elevated {
                    println!("  {}", t(Msg::VersionElevateCancel));
                }
                let _ = io::stdout().flush();
                elevated
            })?;
            app.on_elevate_finished(req, ok);
            // Drop leftover input / resize events from before or during UAC.
            while event::poll(Duration::ZERO).unwrap_or(false) {
                let _ = event::read();
            }
            // Critical: reset ratatui's diff buffer, then paint a full frame.
            // Without this, the blank alt-screen only receives sparse cell updates.
            if let Ok(size) = terminal.size() {
                let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
                let _ = terminal.resize(area);
            }
            terminal.clear()?;
            terminal.draw(|f| app.draw(f))?;
        }
    }

    Ok(TuiOutcome {
        user_quit: app.should_quit,
        run_uninstall: app.run_uninstall,
        elevate_uninstall: app.elevate_uninstall,
        uninstall_scope: app.uninstall_scope,
    })
}
