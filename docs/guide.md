# cursor-cleanup User Guide

Version: **v0.5.4** (Cursor-only; clean/uninstall pick IDE / CLI / both + confirm before force-quit / restore)

UI languages: **Traditional Chinese (default)**, Simplified Chinese, English  
(`--lang zh-TW|zh-CN|en`)

## vs ThendCN/vscode-cursor-cleanup

Upstream: [ThendCN/vscode-cursor-cleanup](https://github.com/ThendCN/vscode-cursor-cleanup) ¡ª macOS Bash scripts for interactive cache / `state.vscdb` cleanup with ZIP backup during clean.

| | Upstream | This project |
|--|----------|--------------|
| Form | `.sh` | Rust single binary |
| OS | macOS-focused | Windows / macOS / Linux |
| UX | `read` prompts | Double-click TUI ¡ª mouse click + keyboard + Back |
| i18n | Chinese script text | zh-TW (default) / zh-CN / en |
| Plans | Steps 1¨C6 | Conservative / Standard / Deep / Custom |
| Backup | Inside cleanup ZIP | Dedicated Backup (config ¡À `state.vscdb`) |
| Restore | Manual unzip in docs | Dedicated Restore ¡ª pick a folder |
| Release | Clone scripts | GitHub Actions multi-platform binaries |

Same mission, but **Cursor-only**: safe tiered cleanup without touching project code or installed extensions.

## Two ways to run

### 1. Double-click (easiest)

1. Download from [Releases](https://github.com/911218sky/vscode-cursor-cleanup/releases)  
   Windows: **`cursor-cleanup.exe`** (or `cursor-cleanup-x86_64-pc-windows-msvc.zip`)
2. If you got the zip, unzip to get `cursor-cleanup.exe`
3. **Double-click** ¡ú terminal opens
4. Pick language (default Traditional Chinese)
5. Use **mouse click** or **¡ü¡ý + Enter**; each step has **Esc / ¡û Back**
6. Main menu:
   - **Clean** ¡ª pick IDE / CLI / both, then caches / history / state.vscdb (or CLI equivalents)
   - **Backup** ¡ª config only, or config + `state.vscdb` (recommended)
   - **Restore / delete** ¡ª restore one backup, or multi-select delete under `backups/`
   - **Uninstall** ¡ª pick IDE / CLI / both; IDE uses official Uninstall.exe (Windows, may request UAC) then wipes leftovers for that scope
7. For Clean, after choosing scope, pick a plan:
   - **Conservative** ¡ª safe caches only
   - **Standard** ¡ª safe + edit history
   - **Deep** ¡ª includes `state.vscdb`
   - **Custom** ¡ª confirm each item
8. Optionally return to the main menu or exit
9. Uninstall removes paths for the chosen scope (e.g. IDE ¡ú `%APPDATA%\Cursor`; CLI ¡ú `~/.cursor`, `cursor-agent`). Project-local `.cursor/` folders are **not** deleted.

UI: `ratatui` + `crossterm` (mouse + keyboard). CLI `--scan` / `--yes` uses plain stdout. Cleanup/backup logic uses the Rust standard library.

### Backup & restore

- Location: `{exe_dir}/backups/cursor-backup-{name}/`
- Name: type your own label, or press Enter for the default timestamp
- Always includes: `settings.json`, `keybindings.json`, `snippets/`
- Optional: `state.vscdb`
- Restore / delete: restore one, or Space-select many and Enter to delete
- Each backup records the Cursor app version; restoring onto an older install asks you to force restore or cancel (newer settings / `state.vscdb` may not be compatible)
- Prefer closing Cursor yourself before restore when convenient

### 2. CLI

```powershell
.\cursor-cleanup.exe
.\cursor-cleanup.exe --lang en --scan --scope ide
.\cursor-cleanup.exe --scan --scope cli
.\cursor-cleanup.exe --yes --scope both
.\cursor-cleanup.exe --help
```

| Flag | Meaning |
|------|---------|
| (none) | Full interactive menu (asks IDE / CLI / Both before clean or backup) |
| `--lang zh-TW\|zh-CN\|en` | UI language (default `zh-TW`) |
| `--scope ide\|cli\|both` | Data scope for `--scan` / `--yes` (default `both`) |
| `--scan` | Scan sizes only |
| `--yes` | Non-interactive, safe items only (skips if scope processes are running) |
| `--no-pause` | Do not pause on exit |

## What gets cleaned?

| Risk | Item | Notes |
|------|------|-------|
| Safe | CachedData, extension VSIX cache, GPU/Cache, logs | Rebuilds automatically; recommended daily |
| Safe | Agent CLI caches (`~/.cursor/statsig-cache.json`, `ai-tracking`) | Does not remove the CLI install |
| Medium | User/History | Local undo history; does not affect Git |
| Medium | Agent CLI state / chats / projects (`~/.cursor/agent-cli-state.json`, `chats`, `projects`) | CLI conversation / project state |
| High | state.vscdb | AI chats, recent files, UI state |
| High | Agent CLI `auth.json` | Logs out `agent` CLI |

**Never deletes** project source code, installed extension binaries, or `%LOCALAPPDATA%\cursor-agent` (CLI install).

Force-quit also stops Agent CLI processes (`agent` / `cursor-agent`). Full backup (config + state) also copies `auth.json`.

## Data paths

| Platform | Cursor IDE | Agent CLI home |
|----------|------------|----------------|
| Windows | `%APPDATA%\Cursor` | `%USERPROFILE%\.cursor` |
| macOS | `~/Library/Application Support/Cursor` | `~/.cursor` |
| Linux | `~/.config/Cursor` | `~/.cursor` |

## Recommendations

1. Quit Cursor before cleaning (avoids locked files) ¡ª the TUI will ask before force-quit
2. Prefer **Conservative** for routine use
3. Only use **Deep** when `state.vscdb` is huge (e.g. > 1GB) and the app feels slow ¡ª backup first

Note: Deep clean removes the entire `state.vscdb` and Agent CLI `auth.json`. It cannot selectively purge only archived AI chats while keeping the chat list. Prefer Cursor¡¯s built-in **Delete Old Chats** / **GC Agent KV Blobs** for that.

## Build from source

```bash
git clone https://github.com/911218sky/vscode-cursor-cleanup.git
cd vscode-cursor-cleanup
cargo build --release
# Windows output used locally: dist/cursor-cleanup.exe
copy /Y target\release\cursor-cleanup.exe dist\
```
