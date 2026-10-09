mod app;
mod terminal;
mod theme;
mod widgets;

use app::App;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};
use std::time::Duration;
use terminal::TerminalGuard;

/// Result of an interactive TUI session.
pub struct TuiOutcome {
    pub did_work: bool,
    /// User chose Exit / Esc from root screens, or Ctrl+C.
    pub user_quit: bool,
}

/// Run the full interactive TUI.
pub fn run(skip_lang: bool) -> io::Result<TuiOutcome> {
    let _guard = TerminalGuard::enter()?;
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
    }

    Ok(TuiOutcome {
        did_work: app.did_work,
        user_quit: app.should_quit,
    })
}
