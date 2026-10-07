# cursor-cleanup User Guide

Version: **v0.3.0** (Rust interactive TUI with mouse support)

UI languages: **Traditional Chinese (default)**, Simplified Chinese, English  
(`--lang zh-TW|zh-CN|en`)

## vs ThendCN/vscode-cursor-cleanup

Upstream: [ThendCN/vscode-cursor-cleanup](https://github.com/ThendCN/vscode-cursor-cleanup) — macOS Bash scripts for interactive cache / `state.vscdb` cleanup with ZIP backup during clean.

| | Upstream | This project |
|--|----------|--------------|
| Form | `.sh` | Rust single binary |
| OS | macOS-focused | Windows / macOS / Linux |
| UX | `read` prompts | Double-click TUI — mouse click + keyboard + Back |
| i18n | Chinese script text | zh-TW (default) / zh-CN / en |
| Plans | Steps 1–6 | Conservative / Standard / Deep / Custom |
| Backup | Inside cleanup ZIP | Dedicated Backup (config ± `state.vscdb`) |
| Restore | Manual unzip in docs | Dedicated Restore — pick a folder |
| Release | Clone scripts | GitHub Actions multi-platform binaries |

Same mission: safe tiered cleanup without touching project code or installed extensions.

## Two ways to run

### 1. Double-click (easiest)

1. Download from [Releases](https://github.com/911218sky/vscode-cursor-cleanup/releases)  
   Windows: **`cursor-cleanup.exe`** (or `cursor-cleanup-x86_64-pc-windows-msvc.zip`)
2. If you got the zip, unzip to get `cursor-cleanup.exe`
3. **Double-click** → terminal opens
4. Pick language (default Traditional Chinese)
5. Use **mouse click** or **↑↓ + Enter**; each step has **Esc / ← Back**
6. Main menu:
   - **Clean** — caches / history / state.vscdb
   - **Backup** — config only, or config + `state.vscdb` (recommended)
   - **Restore / delete** — restore one backup, or multi-select delete under `backups/`
7. For Clean, choose a plan:
   - **Conservative** — safe caches only
   - **Standard** — safe + edit history
   - **Deep** — includes `state.vscdb`
   - **Custom** — confirm each item
8. Optionally return to the main menu or exit
9. Press **Enter** to close

UI: `ratatui` + `crossterm` (mouse + keyboard). CLI `--scan` / `--yes` uses plain stdout. Cleanup/backup logic uses the Rust standard library.

### Backup & restore

- Location: `{exe_dir}/backups/{cursor|vscode}-backup-{name}/`
- Name: type your own label, or press Enter for the default timestamp
- Always includes: `settings.json`, `keybindings.json`, `snippets/`
- Optional: `state.vscdb`
- Restore / delete: restore one, or Space-select many and Enter to delete
- Prefer closing the editor yourself before restore when convenient

### 2. CLI

```powershell
.\cursor-cleanup.exe
.\cursor-cleanup.exe --lang en --scan --app cursor
.\cursor-cleanup.exe --app cursor --yes
.\cursor-cleanup.exe --help
```

| Flag | Meaning |
|------|---------|
| (none) | Full interactive menu |
| `--lang zh-TW\|zh-CN\|en` | UI language (default `zh-TW`) |
| `--app cursor\|vscode\|both` | Target app |
| `--scan` | Scan sizes only |
| `--yes` | Non-interactive, safe items only (requires `--app`; skips if app running) |
| `--no-pause` | Do not pause on exit |

## What gets cleaned?

| Risk | Item | Notes |
|------|------|-------|
| Safe | CachedData, extension VSIX cache, GPU/Cache, logs | Rebuilds automatically; recommended daily |
| Medium | User/History | Local undo history; does not affect Git |
| High | state.vscdb | AI chats, recent files, UI state |

**Never deletes** project source code or installed extension binaries.

## Data paths

| Platform | Cursor | VSCode |
|----------|--------|--------|
| Windows | `%APPDATA%\Cursor` | `%APPDATA%\Code` |
| macOS | `~/Library/Application Support/Cursor` | `~/Library/Application Support/Code` |
| Linux | `~/.config/Cursor` | `~/.config/Code` |

## Recommendations

1. Quit Cursor / VSCode before cleaning (avoids locked files)
2. Prefer **Conservative** for routine use
3. Only use **Deep** when `state.vscdb` is huge (e.g. > 1GB) and the app feels slow — backup first

Note: Deep clean removes the entire `state.vscdb`. It cannot selectively purge only archived AI chats while keeping the chat list. Prefer Cursor’s built-in **Delete Old Chats** / **GC Agent KV Blobs** for that.

## Build from source

```bash
git clone https://github.com/911218sky/vscode-cursor-cleanup.git
cd vscode-cursor-cleanup
cargo build --release
# Windows output used locally: dist/cursor-cleanup.exe
copy /Y target\release\cursor-cleanup.exe dist\
```
