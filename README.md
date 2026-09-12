<p align="center">
  <img src="docs/recall-logo.png" alt="recall logo" width="180" />
</p>

# recall

English | [简体中文](README.zh-CN.md)

A lightweight, Warp-Block-style shell history viewer for the terminal.

Inspired by [Warp](https://www.warp.dev/), but designed as a standalone
shell-history and output-recording tool.

`recall` records each command together with its **output**, working directory,
timestamp and exit code, then lets you browse that history in a TUI where every
execution is a distinct *block*. It does not touch your terminal emulator or
reimplement one: it is a shell-side tool that transparently proxies your shell
through a PTY.

<p align="center">
  <img src="docs/recall-display.png" alt="recall display" width="800" />
</p>

## Install

### One-line installer (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/wendaining/recall/master/install.sh | sh
```

The installer detects Linux or macOS and the current CPU architecture, downloads
the matching binary from the latest GitHub Release, and verifies its SHA-256
checksum. It then:

- installs recall into `/usr/local/bin` or `~/.local/bin`;
- configures zsh, bash, or fish for automatic output capture and search;
- creates the default `config.toml` without overwriting an existing one;
- detects existing bash, zsh, fish, and atuin history and asks whether to import
  each source;
- starts recall's PTY proxy automatically in new interactive shells without
  changing terminal-emulator settings.

Automatic output capture is the default. Set `RECALL_PROXY_SETUP=hooks` for the
lighter alternative that installs search and metadata hooks but only captures
output after you run `recall shell`. The legacy values `shell` and `none` remain
aliases for `auto` and `hooks`; explicit `terminal` mode is retained for
existing terminal-managed setups. Set `RECALL_IMPORT_HISTORY` to `yes`, `no`,
or `ask` to control history migration.

### One-line installer (Windows)

```powershell
irm https://raw.githubusercontent.com/wendaining/recall/master/install.ps1 | iex
```

The installer downloads the latest release, verifies its SHA-256 checksum, and
installs `recall.exe` into `%USERPROFILE%\.local\bin` (override with
`RECALL_INSTALL_DIR`). It adds that directory to your user `PATH`, configures
`$PROFILE` for automatic output capture, and creates the default `config.toml`.
It also offers to import detected shell and atuin histories. Set
`RECALL_NO_MODIFY_PROFILE` to skip the profile change,
`RECALL_PROXY_SETUP=hooks` to install hooks without automatic capture, or
`RECALL_IMPORT_HISTORY=yes|no|ask` to control migration prompts.

### Build from source

```sh
cargo build --release
install -Dm755 target/release/recall ~/.local/bin/recall      # Linux/macOS
```

On Windows the binary is `target\release\recall.exe`; copy it somewhere on
`PATH`. Make sure the install directory is on `PATH`.

### Update

```sh
recall update
```

`recall update` downloads the latest stable GitHub Release for the current
platform, shows download progress, verifies its SHA-256 checksum, and replaces
the running binary. Use `recall update --check` to check without installing.
The history TUI checks for stable updates at most once every 24 hours and shows
an available update in its status bar.

### Uninstall

```sh
curl -fsSL https://raw.githubusercontent.com/wendaining/recall/master/uninstall.sh | sh
```

```powershell
irm https://raw.githubusercontent.com/wendaining/recall/master/uninstall.ps1 | iex
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
- **History import**: migrate bash, zsh, fish, PowerShell, or optional atuin
  history into recall without making any of them a runtime dependency.
- **Sensible edge cases**: no-output, interactive/alt-screen, binary and
  redirected commands are classified instead of silently mangled.
- **Secrets filter** and **retention**: obvious secrets are dropped, and stored
  output expires after 30 days by default.

## Requirements

- Linux, macOS, or Windows 10 1809+ (Windows Terminal with PowerShell)
- Rust (to build) — developed against Rust 1.88+
- SQLite is bundled, no system dependency
- zsh, bash or fish on Linux/macOS; PowerShell 7 or Windows PowerShell 5.1 on
  Windows for shell integration

## Setup

> [!note]
>
> The one-line installer completes the shell integration and creates the default
> configuration automatically. The manual steps below are mainly for source
> builds or custom setups.

### 1. Unified shell setup

If you built from source, run the setup command for your shell:

```sh
recall setup zsh       # or: bash, fish, pwsh, powershell
```

This configures automatic output capture by default. recall places a small,
managed bootstrap at the top of the startup file and the integration at the
end. The outer shell hands off to the PTY proxy before loading the rest of the
file; the child shell then loads your configuration once and installs the
hooks after themes, prompts, and PSReadLine are ready.

For search and metadata hooks without automatic capture, use:

```sh
recall setup zsh --mode hooks
```

Run `recall setup zsh --remove` to remove only the blocks managed by recall.
User-written setup remains untouched.

`recall init` remains the low-level option for fully manual setups:

```zsh
# ~/.zshrc
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

```powershell
# $PROFILE
recall init pwsh | Out-String | Invoke-Expression
```

The integration installs command-boundary hooks and an **Alt+R** widget that
opens the TUI and inserts the selected command into your prompt. The key is set
by `[ui].search_key` in the config; see below for details.

Without the proxy, recall still records command metadata in the background. To
capture output, run your shell under the proxy.

#### Choosing the search key

The key is configured in `~/.config/recall/config.toml` as a semantic name that
`recall init` translates for each shell:

```toml
[ui]
search_key = "alt-r"   # alt-r, ctrl-t, or a two-stroke sequence "ctrl-x ctrl-r"
```

Supported forms are `alt-<letter>`, `ctrl-<letter>`, `ctrl-space`, or a
space-separated sequence such as `"ctrl-x ctrl-r"`. The default is `alt-r`.
Run `recall config` to record and validate a shortcut interactively. Reopen the
shell after changing it; PowerShell sequences are passed to PSReadLine as a
comma-separated chord.

#### macOS: the Option key

Mac keyboards have no `Alt`; the equivalent is `Option` (`⌥`). recall accepts
both the native `Option+R` character (`®`) and the `Meta-R` sequence, so the
search widget works with the default macOS terminal settings. Option-as-Meta
keeps `Alt+R` compatible with terminal conventions:

| Terminal | Setting |
| --- | --- |
| Terminal.app | Settings → Profiles → Keyboard → *Use Option as Meta key* |
| iTerm2 | Preferences → Profiles → Keys → *Left Option Key: Esc+* |
| Ghostty | `macos-option-as-alt = true` |

You can also pick another key, for example `"ctrl-x ctrl-r"` (the installer
offers this on macOS). Run `recall doctor` to confirm the shell integration and
`PATH`.

### 2. Output capture and the PTY proxy

The installer and `recall setup` start your usual shell inside recall's PTY
proxy automatically. No terminal-emulator configuration is needed. Set
`RECALL_PROXY=0` before starting a new shell to bypass automatic capture while
keeping the search and metadata hooks.

In hooks-only or fully manual setups, start a wrapped shell when output capture
is needed:

```sh
recall shell
```

Existing terminal-managed setups may continue launching recall directly, but
this is an advanced compatibility path rather than the recommended setup:

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
Background database errors are written to `recall-errors.log` beside the recall
database instead of being injected into the interactive terminal. Run
`recall doctor` to see the exact database and error-log paths.

On Windows the proxy runs the shell under ConPTY. When neither `proxy.shell` nor
`--shell` is set, it detects the shell you launched `recall shell` from (walking
the parent process chain), then falls back to `pwsh`, `powershell`, and
`%COMSPEC%`. Override with `--shell` or `proxy.shell`, for example
`recall shell --shell cmd`. The macOS-only `-l` login flag is not used on
Windows.

### 3. Import existing history (optional)

```sh
recall import history zsh
recall import history bash --path ~/archives/bash_history
recall import history fish
recall import history pwsh

recall import atuin             # optional adapter; atuin need not be installed
recall import atuin --days 30
recall import atuin --path /path/to/history.db
```

Without `--path`, recall uses the selected shell's standard history location or
atuin's standard database location. Imported blocks contain metadata but no
output. Generic source provenance makes repeated imports safe, while preserving
duplicate commands that represent separate executions.

## Usage

Open the TUI with `recall` (or the Alt+R widget). Type to search; press `F1`
inside the TUI for the full list of key bindings.

> [!note]
>
> `Tab` and `Ctrl+Enter` need the shell widget (`recall search --cmd-only`).
> `Ctrl+Enter` requires a terminal emulator that reports it distinctly; use
> `Ctrl+E` otherwise (on Windows Terminal, use `Ctrl+E`).

Other commands:

```sh
recall search --cmd-only   # print the selection (used by the zsh widget)
recall setup [shell]       # configure automatic capture and shell hooks
recall doctor              # diagnose config, databases and clipboard
recall prune               # drop output older than the retention window
recall config              # open the interactive settings TUI
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
> Run `recall config` to review capture rules before recording sensitive output.
> `recall config path`, `recall config show`, and `recall config default` remain
> available to locate the file, inspect the active settings, and print a template.

### Interactive configuration

```sh
recall config
```

The settings TUI manages the search shortcut, secret and interactive-command
filtering, command/output exclusion regexes, display preferences, and shell
integration. Changes are validated and saved atomically as you make them;
existing TOML comments and unrelated settings are retained. Shortcut changes
take effect in a new shell.

- `Tab` / `Shift+Tab`: change category
- `Up` / `Down`: choose a setting
- `Left` / `Right`: adjust display values or timestamp presets
- `Space`: toggle a setting
- `Enter`: edit, record, or apply
- `a` / `e` / `d`: add, edit, or delete a custom capture rule
- `F1`: help; `Esc` / `q`: back or quit

`~/.config/recall/config.toml` on Linux/macOS and
`%APPDATA%\recall\config.toml` on Windows (all fields optional;
`recall config default` prints a full example). `RECALL_CONFIG` overrides the
path. On Windows the database defaults to `%LOCALAPPDATA%\recall\recall.db`.

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

[ui]
search_key = "alt-r"         # key that opens recall (alt-r, ctrl-t, "ctrl-x ctrl-r")
list_width_pct = 42          # initial list pane width; resizing in the TUI persists
preview_lines = 4            # output lines shown for each result
date_format = "%Y-%m-%d %H:%M:%S"
```

## How it works

```
terminal emulator ──▶ recall proxy (PTY/ConPTY) ──▶ shell (zsh/bash/fish/pwsh)
              │  byte stream: captured output + in-band OSC markers
              ▼
       recall.db (SQLite, WAL)  ◀── recall TUI
```

- The proxy spawns your shell on a PTY (ConPTY on Windows) and forwards bytes in
  both directions, so the terminal experience is unchanged.
- The shell integration writes an in-band start marker carrying
  `{command, cwd, start}` as a private OSC sequence (`ESC ] 9999 ; {...} BEL`)
  before the command runs, and a matching end marker with the exit code before
  the next prompt is drawn.
- The proxy parses and strips these markers, so command boundaries are exact and
  the prompt is never captured. There is no side channel or socket, which
  keeps recall shell- and OS-agnostic.
- Output is ANSI-stripped, classified, capped, zstd-compressed and stored in
  SQLite together with self-contained command metadata. Optional imports are
  tracked in a separate, source-neutral provenance table.

## Compatibility

recall is terminal-agnostic: it uses standard ANSI/OSC sequences and renders
with crossterm, so it works in any VT-compatible terminal. The only per-terminal
differences are clipboard support and whether `Ctrl+Enter` can be reported
distinctly (`Ctrl+E` is the fallback).

| Platform | Shells | Terminal emulator | Notes |
| --- | --- | --- | --- |
| Linux | zsh, bash, fish | any VT-compatible (Konsole, GNOME Terminal, Ghostty, …) | full support |
| macOS | zsh, bash, fish | any VT-compatible (Terminal.app, iTerm2, Ghostty, …) | Terminal.app: clipboard via `pbcopy`, use `Ctrl+E` to execute |
| Windows | PowerShell 7, Windows PowerShell 5.1 | Windows Terminal (ConPTY) | clipboard via native `arboard`/`clip`; use `Ctrl+E` to execute |

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
