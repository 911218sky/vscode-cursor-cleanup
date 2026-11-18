use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use std::io::{self, stdout, Stdout};

pub struct TerminalGuard {
    pub stdout: Stdout,
}

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        stdout.execute(EnterAlternateScreen)?;
        stdout.execute(EnableMouseCapture)?;
        stdout.execute(Hide)?;
        Ok(Self { stdout })
    }

    /// Temporarily leave the TUI (for UAC / external prompts), run `f`, then restore.
    pub fn suspend<R>(&mut self, f: impl FnOnce() -> R) -> io::Result<R> {
        let _ = disable_raw_mode();
        let _ = self.stdout.execute(DisableMouseCapture);
        let _ = self.stdout.execute(Show);
        let _ = self.stdout.execute(LeaveAlternateScreen);
        let result = f();
        enable_raw_mode()?;
        self.stdout.execute(EnterAlternateScreen)?;
        self.stdout.execute(EnableMouseCapture)?;
        self.stdout.execute(Hide)?;
        Ok(result)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = self.stdout.execute(DisableMouseCapture);
        let _ = self.stdout.execute(Show);
        let _ = self.stdout.execute(LeaveAlternateScreen);
    }
}
