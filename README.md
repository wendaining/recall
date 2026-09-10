# recall

A lightweight, Warp-Block-style shell history viewer for the terminal.

Inspired from [Warp](https://www.warp.dev/) and [atuin](https://github.com/atuinsh/atuin).

`recall` records each command together with its **output**, working directory,
timestamp and exit code, then lets you browse that history in a TUI where every
execution is a distinct *block*. It does not touch your terminal emulator or
reimplement one: it is a shell-side tool that transparently proxies your shell
through a PTY.

<img zoom="35%" alt="recall-display" src="https://github.com/user-attachments/assets/835ef068-b4d9-41e2-b5a6-738d5d58f01f" />

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

## Build

```sh
cargo build --release
install -Dm755 target/release/recall ~/.local/bin/recall
```

Make sure `~/.local/bin` is on `PATH`.

## Setup

> [!note]
>
> You can clone this Repo and tell your Agent:
>
> ```text
> Read the Setup part of README.md file and set it up for me.
> ```

### 1. Shell integration

Add the matching line to your shell's startup file:

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

### 2. Run under the proxy

Either start a wrapped shell manually:

```sh
recall shell
```

or make kitty do it for you by adding to `~/.config/kitty/kitty.conf`:

```
shell /home/you/.local/bin/recall shell
```

`recall shell` falls back to a plain shell if the terminal is not a TTY or if
`RECALL_PROXY=0` is set.

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
| `Ctrl+E` | execute (fallback for terminals without the kitty protocol) |
| `Ctrl+Y` / `y` | copy command |
| `Ctrl+O` / `Y` | copy output |
| `PgUp` / `PgDn`, `Home` / `End` | scroll output |
| `Esc` | clear search / leave detail |
| `q` / `Ctrl+C` | quit |
| `F1` | toggle help |

> `Tab` and `Ctrl+Enter` need the shell widget (`recall search --cmd-only`).
> `Ctrl+Enter` requires a terminal that reports it distinctly (kitty does).

Other commands:

```sh
recall search --cmd-only   # print the selection (used by the zsh widget)
recall doctor              # diagnose config, databases and clipboard
recall prune               # drop output older than the retention window
recall config path|show|default
recall uuid
```

## Configuration

`~/.config/recall/config.toml` (all fields optional; `recall config default`
prints a full example). `RECALL_CONFIG` overrides the path.

```toml
[general]
max_output_bytes = 1048576   # per-command output cap (before compression)
strip_ansi = true

[proxy]
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
kitty ──▶ recall proxy (PTY) ──▶ zsh (+ recall.zsh hooks)
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

| Platform | Shells | Terminals | Notes |
| --- | --- | --- | --- |
| Linux | zsh, bash, fish | kitty, Ghostty, Konsole, GNOME Terminal, … | full support |
| macOS | zsh, bash, fish | Ghostty, kitty, Terminal.app, iTerm2 | Terminal.app: clipboard via `pbcopy`, use `Ctrl+E` to execute |
| Windows | — | Windows Terminal | build-only for now; use WSL for full functionality |

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
