use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use std::io::{self, stdout, Stdout, Write};

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
    ///
    /// After UAC the alternate screen is blank, but ratatui still holds the previous
    /// buffer — a differential redraw would only paint "changed" cells and look empty
    /// / garbled. Callers must `terminal.clear()` + full `draw` after this returns.
    pub fn suspend<R>(&mut self, f: impl FnOnce() -> R) -> io::Result<R> {
        let _ = disable_raw_mode();
        let _ = self.stdout.execute(DisableMouseCapture);
        let _ = self.stdout.execute(Show);
        let _ = self.stdout.execute(LeaveAlternateScreen);
        let _ = self.stdout.flush();
        let result = f();
        enable_raw_mode()?;
        self.stdout.execute(EnterAlternateScreen)?;
        // Wipe the freshly entered alt screen so stale glyphs from the main buffer
        // (or a resized console after UAC) cannot leak into the next frame.
        self.stdout.execute(Clear(ClearType::All))?;
        self.stdout.execute(MoveTo(0, 0))?;
        self.stdout.execute(EnableMouseCapture)?;
        self.stdout.execute(Hide)?;
        self.stdout.flush()?;
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
