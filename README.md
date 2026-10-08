# cursor-cleanup

<div align="center">

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)
![Lang](https://img.shields.io/badge/UI-zh--TW%20%7C%20zh--CN%20%7C%20en-blue.svg)
![Version](https://img.shields.io/badge/version-0.4.0-brightgreen.svg)

**雙擊即可用的互動清理工具** · 也支援 CLI  
預設介面：**繁體中文**（可切換簡體／English）

清理 Cursor 快取與膨脹的 `state.vscdb`

[English](./README.en.md) · [简体中文](./README.zh-CN.md) · [Docs (EN)](./docs/guide.md)

</div>

---

基於 [ThendCN/vscode-cursor-cleanup](https://github.com/ThendCN/vscode-cursor-cleanup) 以 Rust 重寫。  
倉庫：https://github.com/911218sky/vscode-cursor-cleanup

## 跟上游差在哪？（亮點）

上游 [ThendCN/vscode-cursor-cleanup](https://github.com/ThendCN/vscode-cursor-cleanup) 是 **macOS Bash 腳本**，互動清理 VSCode／Cursor 快取與 `state.vscdb`，並用 ZIP 備份。

本專案改為 **僅支援 Cursor**，並在同一類問題上做了這些升級：

| | 上游 ThendCN | 本專案 |
|--|--------------|-------------------|
| 形態 | `.sh` 腳本 | **單一 exe／二進位**（Rust） |
| 平台 | 主要 **macOS** | **Windows / macOS / Linux** |
| 使用方式 | 終端跑腳本、設 alias | **雙擊即可**，也可 CLI |
| 互動 UI | `read` 問答 | **方向鍵選單** + 各步可「返回」 |
| 語言 | 中文腳本文案 | **繁中（預設）／簡中／英文** |
| 清理方案 | 逐步確認 1–6 | **保守／標準／深度／自訂** |
| 備份 | 清理流程內 ZIP 到桌面 | **獨立「備份」**，可含設定 ± `state.vscdb` |
| 恢復 | 文件裡手動 `unzip`／複製 | **恢復／刪除備份**，可選一份恢復或勾選多份刪除 |
| 發布 | 自行 clone 腳本 | **GitHub Actions** 多平台 Release |
| 路徑 | 曾硬編碼使用者目錄 | 依系統解析 `%APPDATA%` / Application Support 等 |

### 繼承自上游的核心價值

- 安全分級清理（快取 → 歷史 → `state.vscdb`）
- 選方案後直接清理（備份走獨立選單，不中途再問）
- 不碰專案程式碼與已安裝擴充本體

### 本專案額外亮點（一句話）

> **跨平台、雙擊就能用的清理＋備份／選份恢復工具，預設繁中，Release 直接下載。**

## 快速開始（Windows）

1. 開啟 [Releases](https://github.com/911218sky/vscode-cursor-cleanup/releases)
2. 下載 **`cursor-cleanup.exe`**（或 zip 再解壓）
3. **雙擊 `cursor-cleanup.exe`**
4. 先選語言（預設繁中）→ ↑↓ + Enter 操作（可「← 返回」）
5. 結束後按 **Enter** 關閉視窗

## CLI

```powershell
cursor-cleanup.exe
cursor-cleanup.exe --lang zh-CN
cursor-cleanup.exe --lang en --scan
cursor-cleanup.exe --yes
```

| 參數 | 說明 |
|------|------|
| `--lang zh-TW\|zh-CN\|en` | 語言（預設 `zh-TW`） |
| `--scan` | 只掃描 |
| `--yes` | 自動清安全項（Cursor 執行中會略過） |
| `--no-pause` | 結束不暫停 |

## 清什麼？

| 風險 | 內容 |
|------|------|
| 安全 | CachedData、擴充快取、GPU/Cache、logs |
| 中等 | 本機編輯歷史 History |
| 高危 | `state.vscdb`（AI 聊天／最近檔案／UI） |

不刪：專案程式碼、已安裝擴充本體。

## 文件

- [docs/guide.md](./docs/guide.md) — 英文完整指南
- [README.en.md](./README.en.md) / [README.zh-CN.md](./README.zh-CN.md)

## 建置

```bash
cargo build --release
# Windows：複製到 dist\
mkdir -p dist 2>nul & copy /Y target\release\cursor-cleanup.exe dist\
```

## 授權

MIT — [LICENSE](LICENSE)  
原作者 [ThendCN](https://github.com/ThendCN)；本倉庫 [911218sky](https://github.com/911218sky)。

