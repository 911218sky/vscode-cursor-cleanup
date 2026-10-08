# cursor-cleanup

<div align="center">

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Version](https://img.shields.io/badge/version-0.4.0-brightgreen.svg)

**双击即可用的互动清理工具** · 也支持 CLI  
默认界面语言：**繁体中文**；可选简体 / English

清理 Cursor 缓存与膨胀的 `state.vscdb`

[繁體中文](./README.md) · [English](./README.en.md) · [文档(英文)](./docs/guide.md)

</div>

---

基于 [ThendCN/vscode-cursor-cleanup](https://github.com/ThendCN/vscode-cursor-cleanup) 用 Rust 重写。  
仓库：https://github.com/911218sky/vscode-cursor-cleanup

## 跟上游差在哪？（亮点）

上游 [ThendCN/vscode-cursor-cleanup](https://github.com/ThendCN/vscode-cursor-cleanup) 是 **macOS Bash 脚本**，互动清理 VSCode／Cursor 缓存与 `state.vscdb`，并用 ZIP 备份。

本项目改为 **仅支持 Cursor**，并在同一类问题上做了这些升级：

| | 上游 ThendCN | 本项目 |
|--|--------------|-------------------|
| 形态 | `.sh` 脚本 | **单一 exe／二进制**（Rust） |
| 平台 | 主要 **macOS** | **Windows / macOS / Linux** |
| 使用方式 | 终端跑脚本、设 alias | **双击即可**，也可 CLI |
| 互动 UI | `read` 问答 | **方向键菜单** + 各步可「返回」 |
| 语言 | 中文脚本文案 | **繁中（默认）／简中／英文** |
| 清理方案 | 逐步确认 1–6 | **保守／标准／深度／自定义** |
| 备份 | 清理流程内 ZIP 到桌面 | **独立「备份」**，可含设置 ± `state.vscdb` |
| 恢复 | 文档里手动 `unzip`／复制 | **恢复／删除备份**，可选一份恢复或勾选多份删除 |
| 发布 | 自行 clone 脚本 | **GitHub Actions** 多平台 Release |
| 路径 | 曾硬编码用户目录 | 依系统解析 `%APPDATA%` / Application Support 等 |

### 继承自上游的核心价值

- 安全分级清理（缓存 → 历史 → `state.vscdb`）
- 选方案后直接清理（备份走独立菜单，不中途再问）
- 不碰项目代码与已安装扩展本体

### 本项目额外亮点（一句话）

> **跨平台、双击就能用的清理＋备份／选份恢复工具，默认繁中，Release 直接下载。**

## 快速开始（Windows）

1. 打开 [Releases](https://github.com/911218sky/vscode-cursor-cleanup/releases)
2. 下载 **`cursor-cleanup.exe`**（或 zip 再解压）
3. **双击 `cursor-cleanup.exe`**
4. 先选语言 → ↑↓ + Enter（可「← 返回」）
5. 结束后按 **Enter** 关闭窗口

## CLI

```powershell
cursor-cleanup.exe --lang zh-CN
cursor-cleanup.exe --lang zh-CN --scan
cursor-cleanup.exe --yes
```

| 参数 | 说明 |
|------|------|
| `--lang zh-TW\|zh-CN\|en` | 语言（默认 `zh-TW`） |
| `--scan` | 只扫描 |
| `--yes` | 自动清安全项（Cursor 运行中会跳过） |
| `--no-pause` | 结束不暂停 |

## 清什么？

| 风险 | 内容 |
|------|------|
| 安全 | CachedData、扩展缓存、GPU/Cache、logs |
| 中等 | 本地编辑历史 History |
| 高危 | `state.vscdb`（AI 聊天／最近文件／UI） |

不删：项目代码、已装扩展本体。

## 文档

完整说明见英文：[docs/guide.md](./docs/guide.md)

## 构建

```bash
cargo build --release
# Windows：复制到 dist\
mkdir -p dist 2>nul & copy /Y target\release\cursor-cleanup.exe dist\
```

## 许可证

MIT — 见 [LICENSE](LICENSE)

