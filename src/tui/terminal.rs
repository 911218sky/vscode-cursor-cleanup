use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use std::io::{self, stdout, Stdout, Write};

/// Force UTF-8 + VT processing on the attached Windows console.
///
/// Elevated ("Run as administrator") consoles often start on the OEM code page
/// without VT mode; without this, UTF-8 Chinese and ratatui ANSI can garble.
pub fn prepare_windows_console() {
    #[cfg(windows)]
    {
        const CP_UTF8: u32 = 65001;
        const STD_OUTPUT_HANDLE: i32 = -11;
        const ENABLE_PROCESSED_OUTPUT: u32 = 0x0001;
        const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
        const INVALID_HANDLE_VALUE: isize = -1;

        #[link(name = "kernel32")]
        extern "system" {
            fn SetConsoleOutputCP(w_code_page_id: u32) -> i32;
            fn SetConsoleCP(w_code_page_id: u32) -> i32;
            fn GetStdHandle(n_std_handle: i32) -> *mut std::ffi::c_void;
            fn GetConsoleMode(h_console_handle: *mut std::ffi::c_void, lp_mode: *mut u32) -> i32;
            fn SetConsoleMode(h_console_handle: *mut std::ffi::c_void, dw_mode: u32) -> i32;
        }

        unsafe {
            let _ = SetConsoleOutputCP(CP_UTF8);
            let _ = SetConsoleCP(CP_UTF8);

            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if !handle.is_null() && handle as isize != INVALID_HANDLE_VALUE {
                let mut mode = 0u32;
                if GetConsoleMode(handle, &mut mode) != 0 {
                    let _ = SetConsoleMode(
                        handle,
                        mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
                    );
                }
            }
        }
    }
}

pub struct TerminalGuard {
    pub stdout: Stdout,
}

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        prepare_windows_console();
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
        // UAC / new consoles can reset CP / VT — re-apply before redrawing.
        prepare_windows_console();
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
