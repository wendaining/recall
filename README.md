# recall

English | [简体中文](README.zh-CN.md)

A lightweight, Warp-Block-style shell history viewer for the terminal.

Inspired from [Warp](https://www.warp.dev/) and [atuin](https://github.com/atuinsh/atuin).

`recall` records each command together with its **output**, working directory,
timestamp and exit code, then lets you browse that history in a TUI where every
execution is a distinct *block*. It does not touch your terminal emulator or
reimplement one: it is a shell-side tool that transparently proxies your shell
through a PTY.

<img zoom="35%" alt="recall-display" src="https://github.com/user-attachments/assets/835ef068-b4d9-41e2-b5a6-738d5d58f01f" />

## Install

### One-line installer (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/wendaining/recall/master/install.sh | sh
```

The installer detects Linux or macOS and the current CPU architecture, downloads
the matching binary from the latest GitHub Release, and verifies its SHA-256
checksum. It then:

- installs recall into `/usr/local/bin` or `~/.local/bin`;
- adds the shell integration to zsh, bash, or fish automatically;
- creates the default `config.toml` without overwriting an existing one; and
- asks how new terminals should start recall's PTY proxy for automatic output
  capture. Terminal emulator configuration is recommended; shell startup is
  available as a fallback.

For a non-interactive installation, set `RECALL_PROXY_SETUP` to `terminal`,
`shell`, or `none` on the `sh` command.

### Build from source

```sh
cargo build --release
install -Dm755 target/release/recall ~/.local/bin/recall
```

Make sure `~/.local/bin` is on `PATH`.

### Uninstall

```sh
curl -fsSL https://raw.githubusercontent.com/wendaining/recall/master/uninstall.sh | sh
```

The uninstaller removes the binary and only the shell setup managed by the
installer. It keeps your configuration and command history. If you installed to
a custom `RECALL_INSTALL_DIR`, pass the same variable to the uninstall command.

## Features

- **Output capture** via a PTY proxy, so colors, `isatty`, and interactive
  programs keep working.
- **Block UI**: every execution is a block with a separator, metadata header,
  command line and output preview.
- **Search** across commands *and* output (SQLite FTS5 with a trigram tokenizer,
  so CJK substring search works).
- **Copy** the selected command or its output to the clipboard through a
  pluggable backend (Wayland `wl-copy`, X11 `xclip`/`xsel`, native `arboard`,
  or OSC 52 over SSH/tmux).
- **Rerun** the selected command straight from the TUI.
- **Reuse atuin**: import existing atuin history as metadata; recall only adds
  what atuin does not store (output).
- **Sensible edge cases**: no-output, interactive/alt-screen, binary and
  redirected commands are classified instead of silently mangled.
- **Secrets filter** and **retention**: obvious secrets are dropped, and stored
  output expires after 30 days by default.

## Requirements

- Linux or macOS (Windows build support is planned, not yet functional)
- Rust (to build) — developed against Rust 1.88+
- SQLite is bundled, no system dependency
- zsh, bash or fish for shell integration

## Setup

> [!note]
>
> The one-line installer completes the shell integration and creates the default
> configuration automatically. The manual steps below are mainly for source
> builds or custom setups.

### 1. Shell integration

If you built from source, add the matching line to your shell's startup file:

```zsh
# ~/.zshrc (after `eval "$(atuin init zsh)"` if you use atuin)
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

This installs the capture hooks and an **Alt+R** widget that opens the TUI and
inserts the selected command into your prompt. Override the key with
`RECALL_KEY` (zsh), or rebind in bash/fish.

Without the proxy, recall still records command metadata in the background. To
capture output, run your shell under the proxy.

#### macOS: the Option key

Mac keyboards have no `Alt`; the equivalent is `Option` (`⌥`). By default most
macOS terminals treat `Option` as a compose key, so `Option+R` types `®` instead
of sending `Meta-R`, and the widget never opens. Enable Option-as-Meta in your
terminal:

| Terminal | Setting |
| --- | --- |
| Terminal.app | Settings → Profiles → Keyboard → *Use Option as Meta key* |
| iTerm2 | Preferences → Profiles → Keys → *Left Option Key: Esc+* |
| Ghostty | `macos-option-as-alt = true` |

Alternatively bind another key with `RECALL_KEY` before the integration is
loaded (for example `^R` if you do not use atuin, or `^T`):

```zsh
# ~/.zshrc (before `eval "$(recall init zsh)"`)
export RECALL_KEY='^R'
```

Run `recall doctor` to confirm the shell integration and `PATH`.

### 2. Enable output capture with the PTY proxy

The one-line installer offers two setup methods. Configuring your terminal
emulator's startup command is recommended. If your terminal does not provide
that setting, the installer can configure your shell startup file instead.
Either method starts your usual shell inside recall's PTY proxy so recall can
save the output associated with each command.

If you skipped the setup or installed from source, start a wrapped shell
manually:

```sh
recall shell
```

or configure your terminal emulator to launch it as the shell, so every new
window is captured automatically. The setting name varies between emulators
(`shell`, `command`, …), for example:

```
# in your terminal emulator's config file
shell /home/you/.local/bin/recall shell
```

`recall shell` falls back to a plain shell if the terminal is not a TTY, if
`RECALL_PROXY=0` is set, or if the proxy fails to start.

On macOS the proxy starts the shell as a login shell (`zsh -l`) so `~/.zprofile`
and tools such as Homebrew are initialized. Use `--no-login` or set
`proxy.login_shell = false` to opt out. The proxy also prepends its own
directory to the child `PATH`, so the `recall init` hooks keep working even when
a terminal launches `recall shell` before your profile is loaded. If a session
produces output but no command markers, the proxy prints a hint when it exits.

### 3. Import existing atuin history (optional)

```sh
recall import atuin            # all history
recall import atuin --days 30  # only the last 30 days
```

Imported blocks have metadata but no output. Re-running is safe: existing
`atuin_id`s are skipped.

## Usage

Open the TUI with `recall` (or the Alt+R widget):

| Key | Action |
| --- | --- |
| type | search commands and output |
| `↑` / `↓` | move selection (search) / scroll output (detail) |
| `Enter` | switch focus between search and detail |
| `Tab` | edit selected command (insert into the prompt) |
| `Ctrl+Enter` | execute selected command |
| `Ctrl+E` | execute (fallback for terminals that don't report Ctrl+Enter distinctly) |
| `Ctrl+T` | toggle the current block's selection |
| `Ctrl+Y` / `y` | copy command |
| `Ctrl+O` | copy selected commands and outputs chronologically; copy current output if none are selected |
| `Y` | copy current output |
| `PgUp` / `PgDn`, `Home` / `End` | scroll output |
| `Esc` | clear search / leave detail |
| `Ctrl+C` | quit |
| `F1` | toggle help |

> `Tab` and `Ctrl+Enter` need the shell widget (`recall search --cmd-only`).
> `Ctrl+Enter` requires a terminal emulator that reports it distinctly; use
> `Ctrl+E` otherwise.

Other commands:

```sh
recall search --cmd-only   # print the selection (used by the zsh widget)
recall doctor              # diagnose config, databases and clipboard
recall prune               # drop output older than the retention window
recall config path|show|default
recall uuid
```

## Configuration

> [!IMPORTANT]
>
> Review your configuration before enabling the proxy. It controls where the
> database is stored, which command output is recorded, secrets filtering,
> retention, and clipboard behavior. The defaults work out of the box, but a
> deliberate configuration helps avoid retaining noisy or sensitive output.
> Use `recall config path`, `recall config show`, and `recall config default`
> to locate the file, inspect the active settings, and view a complete template.

`~/.config/recall/config.toml` (all fields optional; `recall config default`
prints a full example). `RECALL_CONFIG` overrides the path.

```toml
[general]
max_output_bytes = 1048576   # per-command output cap (before compression)
strip_ansi = true

[proxy]
login_shell = true           # spawn the shell with -l (default: true on macOS)
exclude_output = ["^docker logs", "^ffmpeg", "^tail -f"] # keep metadata, skip output
mark_interactive = true      # skip output of full-screen programs
secrets_filter = true

[retention]
retention_days = 30          # 0 disables expiry
auto_prune = true

[clipboard]
backend = "auto"             # auto | arboard | osc52 | wl-copy | xclip | xsel
```

## How it works

```
terminal emulator ──▶ recall proxy (PTY) ──▶ shell (zsh/bash/fish)
              │  byte stream: captured output + in-band OSC markers
              ▼
       recall.db (SQLite, WAL)  ◀── recall TUI
```

- The proxy spawns your shell on a PTY and forwards bytes in both directions, so
  the terminal experience is unchanged.
- `preexec` writes an in-band start marker carrying `{command, cwd, start}` as a
  private OSC sequence (`ESC ] 9999 ; {...} BEL`); `precmd` writes the matching
  end marker with the exit code before the prompt is drawn.
- The proxy parses and strips these markers, so command boundaries are exact and
  the next prompt is never captured. There is no side channel or socket, which
  keeps recall shell- and OS-agnostic.
- Output is ANSI-stripped, classified, capped, zstd-compressed and stored in
  SQLite. Command metadata is stored redundantly and linked to atuin by
  `atuin_id`.

## Compatibility

recall is terminal-agnostic: it uses standard ANSI/OSC sequences and renders
with crossterm, so it works in any VT-compatible terminal. The only per-terminal
differences are clipboard support and whether `Ctrl+Enter` can be reported
distinctly (`Ctrl+E` is the fallback).

| Platform | Shells | Terminal emulator | Notes |
| --- | --- | --- | --- |
| Linux | zsh, bash, fish | any VT-compatible (Konsole, GNOME Terminal, Ghostty, …) | full support |
| macOS | zsh, bash, fish | any VT-compatible (Terminal.app, iTerm2, Ghostty, …) | Terminal.app: clipboard via `pbcopy`, use `Ctrl+E` to execute |
| Windows | — | any VT-compatible (Windows Terminal, …) | build-only for now; use WSL for full functionality |

Clipboard backends are chosen automatically: `wl-copy` (Wayland), `xclip`/`xsel`
(X11), `pbcopy` (macOS), `clip` (Windows), then native `arboard`, then OSC 52.

## Limitations

- **Redirected output is not captured.** `cmd > file` never reaches the
  terminal, so the proxy cannot see it; such blocks are marked as unavailable.
  Warp has the same limitation.
- Output produced outside the command window (e.g. background jobs) is not
  attributed to a block.
- The proxy must wrap the shell; a plain shell only records metadata.
- Secret detection is best-effort.

## Development

```sh
cargo build
cargo test
cargo fmt
cargo clippy --all-targets
```

See `AGENTS.md` for architecture and conventions.

## License

MIT
