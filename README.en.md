# cursor-cleanup

<div align="center">

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Version](https://img.shields.io/badge/version-0.4.2-brightgreen.svg)

**Double-click interactive cleaner** · also works as CLI  
Default UI language: **Traditional Chinese** (`zh-TW`); also `zh-CN` / `en`

Cleans Cursor caches and bloated `state.vscdb`

[繁體中文](./README.md) · [简体中文](./README.zh-CN.md) · [Docs](./docs/guide.md)

</div>

---

Rust rewrite based on [ThendCN/vscode-cursor-cleanup](https://github.com/ThendCN/vscode-cursor-cleanup).  
Repo: https://github.com/911218sky/vscode-cursor-cleanup

## What’s different from upstream?

Upstream [ThendCN/vscode-cursor-cleanup](https://github.com/ThendCN/vscode-cursor-cleanup) is a **macOS Bash** interactive cleaner for VSCode/Cursor caches and `state.vscdb`, with ZIP backups during cleanup.

This project is **Cursor-only** and adds:

| | Upstream ThendCN | This project |
|--|------------------|--------------|
| Form | Shell scripts (`.sh`) | **Single binary** (Rust) |
| Platforms | **macOS**-focused | **Windows / macOS / Linux** |
| UX | Terminal + `read` prompts | **Double-click** + arrow-key menus + **Back** |
| Language | Chinese script text | **zh-TW (default) / zh-CN / en** |
| Clean plans | Step-by-step 1–6 confirms | **Conservative / Standard / Deep / Custom** |
| Backup | ZIP inside cleanup flow | **Dedicated Backup** (config ± `state.vscdb`) |
| Restore | Manual unzip/copy in docs | **Restore / delete** — restore one or multi-select delete |
| Distribution | Clone & chmod scripts | **GitHub Actions** multi-platform Releases |
| Paths | Hardcoded user paths (fixed upstream-style) | Resolves `%APPDATA%` / Application Support / XDG |

### Kept from upstream

- Risk-tiered cleanup (caches → history → `state.vscdb`)
- Pick a plan and clean immediately (backup is a separate menu)
- Never touches project source or installed extensions

### One-line pitch

> **Cross-platform, double-click cleaner with backup + selectable restore — Traditional Chinese by default, download from Releases.**

## Quick start (Windows)

1. Open [Releases](https://github.com/911218sky/vscode-cursor-cleanup/releases)
2. Download **`cursor-cleanup.exe`** (or the `.zip` and unzip)
3. **Double-click `cursor-cleanup.exe`**
4. Pick language → use ↑↓ + Enter (Back available)
5. Press **Enter** to close when finished

## CLI

```powershell
cursor-cleanup.exe --lang en
cursor-cleanup.exe --lang en --scan
cursor-cleanup.exe --yes
```

| Flag | Meaning |
|------|---------|
| `--lang zh-TW\|zh-CN\|en` | UI language (default `zh-TW`) |
| `--scan` | Scan only |
| `--yes` | Auto-clean safe items (skips if Cursor is running) |
| `--no-pause` | No pause on exit |

## What it cleans

| Risk | Items |
|------|-------|
| Safe | CachedData, extension VSIX cache, GPU/Cache, logs |
| Medium | Local edit History |
| High | `state.vscdb` (AI chats / recent files / UI) |

Does **not** delete project code or installed extensions.

## Docs

English guide: [docs/guide.md](./docs/guide.md)

## Build

```bash
cargo build --release
# Windows: copy to dist\
mkdir dist 2>nul & copy /Y target\release\cursor-cleanup.exe dist\
```

## License

MIT — see [LICENSE](LICENSE)

