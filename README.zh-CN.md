# recall

[English](README.md) | 简体中文

一个轻量、Warp Block 风格的终端 shell 历史查看器。

灵感来自 [Warp](https://www.warp.dev/) 与 [atuin](https://github.com/atuinsh/atuin)。

`recall` 会记录每条命令及其**输出**、工作目录、时间戳和退出码，然后让你在一个
TUI 中浏览这些历史——每次执行都是一个独立的 *block*。它不改动你的终端模拟器，
也不重新实现一个：它是一个 shell 侧工具，通过 PTY 透明地代理你的 shell。

<img zoom="35%" alt="recall-display" src="https://github.com/user-attachments/assets/835ef068-b4d9-41e2-b5a6-738d5d58f01f" />

## 安装

### 一行安装（推荐）

```sh
curl -fsSL https://raw.githubusercontent.com/wendaining/recall/master/install.sh | sh
```

安装脚本会自动识别 Linux 或 macOS 及当前 CPU 架构，从最新的 GitHub Release
下载对应二进制文件，校验 SHA-256 后安装到 `/usr/local/bin` 或
`~/.local/bin`。安装完成后，请按脚本输出的提示配置 shell hook 和配置文件。
脚本还会显示当前配置文件路径及简单的首次使用说明，包括进入 recall 后按 `F1`
查看帮助。

### 从源码构建

```sh
cargo build --release
install -Dm755 target/release/recall ~/.local/bin/recall
```

确保 `~/.local/bin` 在 `PATH` 中。

## 特性

- **输出捕获**：通过 PTY 代理，颜色、`isatty` 判断和交互式程序都保持正常。
- **Block UI**：每次执行是一个 block，带分隔线、元信息头、命令行和输出预览。
- **搜索**：同时搜索命令*和*输出（SQLite FTS5 + trigram 分词器，支持中文子串搜索）。
- **复制**：通过可插拔后端把选中的命令或输出复制到剪贴板（Wayland `wl-copy`、
  X11 `xclip`/`xsel`、原生 `arboard`，或 SSH/tmux 下的 OSC 52）。
- **重跑**：直接在 TUI 中重新执行选中的命令。
- **复用 atuin**：把已有 atuin 历史导入为元数据；recall 只补充 atuin 不存的输出。
- **合理的边界处理**：无输出、交互式/全屏、二进制、被重定向的命令都会被分类标记，
  而不是被悄悄弄乱。
- **密钥过滤**与**保留策略**：明显的密钥会被丢弃，存储的输出默认 30 天后过期。

## 环境要求

- Linux 或 macOS（Windows 仅保证可编译，功能尚未支持）
- Rust（用于构建）——基于 Rust 1.88+ 开发
- SQLite 已内置，无系统依赖
- zsh、bash 或 fish 用于 shell 集成

## 安装配置

> [!note]
>
> 你可以克隆本仓库，然后告诉你的 Agent：
>
> ```text
> Read the Setup part of README.md file and set it up for me.
> ```

### 1. Shell 集成

在对应 shell 的启动文件中加入相应的一行：

```zsh
# ~/.zshrc（如果用了 atuin，放在 `eval "$(atuin init zsh)"` 之后）
eval "$(recall init zsh)"
```

```bash
# ~/.bashrc
eval "$(recall init bash)"
```

```fish
# ~/.config/fish/config.fish
recall init fish | source
```

这会安装捕获钩子，以及一个 **Alt+R** 组件：打开 TUI 并把选中的命令插入到提示符。
zsh 下可用 `RECALL_KEY` 覆盖按键，bash/fish 下可自行重新绑定。

未使用代理时，recall 仍会在后台记录命令元数据。要捕获输出，需要让 shell 运行在
代理之下。

### 2. 在代理下运行

可以手动启动一个被包裹的 shell：

```sh
recall shell
```

或配置终端模拟器，把它作为 shell 启动，这样每个新窗口都会自动捕获输出。不同
模拟器的设置项名称不同（`shell`、`command` 等），例如：

```
# 在你的终端模拟器配置文件中
shell /home/you/.local/bin/recall shell
```

如果终端不是 TTY、设置了 `RECALL_PROXY=0`，或代理启动失败，`recall shell`
会回退到普通 shell。

### 3. 导入已有 atuin 历史（可选）

```sh
recall import atuin            # 全部历史
recall import atuin --days 30  # 仅最近 30 天
```

导入的 block 只有元数据、没有输出。重复运行是安全的：已存在的 `atuin_id` 会被跳过。

## 使用

用 `recall`（或 Alt+R 组件）打开 TUI：

| 按键 | 行为 |
| --- | --- |
| 直接输入 | 搜索命令和输出 |
| `↑` / `↓` | 搜索面板移动选择 / 详情面板滚动输出 |
| `Enter` | 在搜索与详情面板之间切换焦点 |
| `Tab` | 编辑选中命令（插入到提示符） |
| `Ctrl+Enter` | 执行选中命令 |
| `Ctrl+E` | 执行（终端无法区分 Ctrl+Enter 时的回退键） |
| `Ctrl+Y` / `y` | 复制命令 |
| `Ctrl+O` / `Y` | 复制输出 |
| `PgUp` / `PgDn`、`Home` / `End` | 滚动输出 |
| `Esc` | 清空搜索 / 退出详情面板 |
| `q` / `Ctrl+C` | 退出 |
| `F1` | 切换帮助 |

> `Tab` 和 `Ctrl+Enter` 依赖 shell 组件（`recall search --cmd-only`）。
> `Ctrl+Enter` 需要终端模拟器能区分上报；否则请用 `Ctrl+E`。

其他命令：

```sh
recall search --cmd-only   # 打印选择结果（供 shell 组件使用）
recall doctor              # 诊断配置、数据库和剪贴板
recall prune               # 清理超过保留期的输出
recall config path|show|default
recall uuid
```

## 配置

> [!IMPORTANT]
>
> 启用代理前，请务必检查配置文件。它直接决定数据库的存储位置、哪些命令输出会被
> 记录、密钥过滤、数据保留期限和剪贴板行为。默认配置可以直接运行，但主动配置能
> 避免长期保存大量无用输出或敏感内容。可使用 `recall config path`、
> `recall config show` 和 `recall config default` 分别查看配置路径、当前生效配置和
> 完整配置模板。

`~/.config/recall/config.toml`（所有字段均可选；`recall config default`
会打印完整示例）。`RECALL_CONFIG` 可覆盖路径。

```toml
[general]
max_output_bytes = 1048576   # 单条命令输出上限（压缩前）
strip_ansi = true

[proxy]
mark_interactive = true      # 跳过全屏程序的输出
secrets_filter = true

[retention]
retention_days = 30          # 0 表示不过期
auto_prune = true

[clipboard]
backend = "auto"             # auto | arboard | osc52 | wl-copy | xclip | xsel
```

## 工作原理

```
终端模拟器 ──▶ recall proxy (PTY) ──▶ shell (zsh/bash/fish)
              │  字节流：捕获的输出 + 带内 OSC 标记
              ▼
       recall.db (SQLite, WAL)  ◀── recall TUI
```

- 代理在 PTY 上启动你的 shell，并双向转发字节，因此终端体验保持不变。
- `preexec` 以私有 OSC 序列（`ESC ] 9999 ; {...} BEL`）写入带内起始标记，携带
  `{command, cwd, start}`；`precmd` 在提示符绘制前写入带退出码的结束标记。
- 代理解析并剥离这些标记，因此命令边界精确，下一个提示符永远不会被捕获。没有
  旁路通道或 socket，这让 recall 与 shell、操作系统无关。
- 输出经过去 ANSI、分类、限长、zstd 压缩后存入 SQLite。命令元数据冗余存储，并通过
  `atuin_id` 与 atuin 关联。

## 兼容性

recall 与终端无关：它使用标准 ANSI/OSC 序列，并用 crossterm 渲染，因此任何兼容 VT
的终端都能运行。唯一的终端差异是剪贴板支持，以及能否区分上报 `Ctrl+Enter`
（回退键是 `Ctrl+E`）。

| 平台 | Shell | 终端模拟器 | 说明 |
| --- | --- | --- | --- |
| Linux | zsh, bash, fish | 任意兼容 VT（Konsole、GNOME Terminal、Ghostty 等） | 完整支持 |
| macOS | zsh, bash, fish | 任意兼容 VT（Terminal.app、iTerm2、Ghostty 等） | Terminal.app：用 `pbcopy` 复制，用 `Ctrl+E` 执行 |
| Windows | — | 任意兼容 VT（Windows Terminal 等） | 目前仅可编译；完整功能请用 WSL |

剪贴板后端会自动选择：`wl-copy`（Wayland）、`xclip`/`xsel`（X11）、`pbcopy`（macOS）、
`clip`（Windows），然后是原生 `arboard`，最后是 OSC 52。

## 已知限制

- **重定向的输出无法捕获。** `cmd > file` 不会经过终端，代理看不到；这类 block 会
  被标记为不可用。Warp 也有同样的限制。
- 命令窗口之外产生的输出（例如后台任务）不会归属到某个 block。
- 必须由代理包裹 shell；普通 shell 只能记录元数据。
- 密钥检测是尽力而为。

## 开发

```sh
cargo build
cargo test
cargo fmt
cargo clippy --all-targets
```

架构与约定见 `AGENTS.md`。

## 许可证

MIT
