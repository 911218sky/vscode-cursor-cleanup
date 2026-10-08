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

    let mut needs_draw = true;

    loop {
        if needs_draw {
            terminal.draw(|f| app.draw(f))?;
            needs_draw = false;
        }

        if app.should_quit {
            break;
        }

        let timeout = if app.wants_animation() {
            100
        } else {
            500
        };

        if event::poll(Duration::from_millis(timeout))? {
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
            // Mouse / key traffic must not starve ForceQuitting, Progress, Scanning.
            if app.wants_animation() {
                app.tick();
            }
            needs_draw = true;
        } else if app.idle_redraw() {
            app.tick();
            needs_draw = true;
        }
    }

    Ok(TuiOutcome {
        did_work: app.did_work,
        user_quit: app.should_quit,
    })
}
