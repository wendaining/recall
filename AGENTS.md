# AGENTS.md

Guidance for agents and contributors working on `recall`.

## What this is

`recall` is a Warp-Block-style shell history viewer and a complement to atuin.
It captures command **output** (which atuin does not store) by running the shell
under a PTY proxy, stores everything in SQLite, and presents it in a ratatui TUI.

Target platform: Linux + zsh + kitty.

## Commands

```sh
cargo build            # debug build
cargo test             # unit tests (marker filter, classifier, db, secrets)
cargo fmt              # formatting (run before committing)
cargo clippy --all-targets
cargo build --release
```

There is no separate lint script; use `cargo fmt` + `cargo clippy`.

## Architecture

```
src/
  main.rs          module wiring
  cli.rs           clap command definitions
  config.rs        TOML config + default paths (RECALL_CONFIG override)
  model.rs         Block + BlockKind
  util.rs          time/hostname/ANSI helpers
  db/
    schema.rs      SQLite schema v1 + migrations + FTS5 trigram
    queries.rs     insert / search / get / prune
  capture/
    proxy.rs       PTY proxy, control-socket listener, async writer
    protocol.rs    newline-delimited JSON control messages
    marker.rs      streaming private-OSC end-marker stripper
    classifier.rs  ANSI strip + normal/empty/interactive/binary classification
    secrets.rs     best-effort secret detection
  clipboard/       Clipboard trait + arboard / OSC52 / external backends
  tui/
    app.rs         App state + key handling
    ui.rs          ratatui rendering
    mod.rs         terminal setup (renders to stderr) + event loop
  commands/        one module per CLI command
shell/recall.zsh   embedded zsh integration (include_str!)
```

### Capture flow

1. `recall shell` -> `capture::proxy::run` opens a PTY, spawns the shell with
   `RECALL_PROXY_ACTIVE=1`, `RECALL_SOCK=<path>`, `RECALL_SESSION=<id>`.
2. zsh `preexec` sends a `start` message over the socket and waits for ack, so
   the proxy is capturing before the command runs.
3. The proxy forwards PTY bytes to stdout and appends them to the active buffer.
4. zsh `precmd` writes `ESC ] 9999 ; recall-end BEL` (in-band, before the
   prompt) and sends an `end` message with the exit code.
5. `marker::MarkerFilter` strips the marker and stops capture exactly at the
   boundary; `end` finalizes, classifies, and hands the block to a writer thread
   that inserts into SQLite.

The TUI renders to **stderr** on purpose: stdout carries the selected command so
the zsh widget can capture it via `$(recall search --cmd-only)`. `Tab` selects
for editing (exit 0); `Ctrl+Enter` exits with code 2, which tells the widget to
execute immediately.

### Key invariants

- The end marker must be emitted in-band before the prompt, otherwise the next
  prompt is captured. `PROMPT_EOL_MARK` is blanked in proxied shells because it
  is printed before `precmd`.
- Never use `exec {fd}>&- 2>/dev/null` in zsh: `exec` redirections persist and
  this permanently redirects the shell's stderr to `/dev/null`.
- zsh does not word-split unquoted expansions; pass flags as separate arguments
  or arrays.
- Output is stored zstd-compressed; a truncated plain-text projection lives in
  `output_text` for FTS. Keep the two in sync.
- DB access uses WAL + `busy_timeout`; multiple processes may write.

## Conventions

- Conventional Commits, small incremental commits (`feat:`, `fix:`, `docs:`,
  `refactor:`, `test:`).
- No comments unless they explain non-obvious behavior.
- Keep `cargo fmt` and `cargo clippy --all-targets` clean.
- Prefer adding a focused unit test next to the module (`#[cfg(test)]`).
