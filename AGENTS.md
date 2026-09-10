# AGENTS.md

Guidance for agents and contributors working on `recall`.

## What this is

`recall` is a Warp-Block-style shell history viewer and a complement to atuin.
It captures command **output** (which atuin does not store) by running the shell
under a PTY proxy, stores everything in SQLite, and presents it in a ratatui TUI.

Target platform: Linux and macOS, zsh/bash/fish, any VT-compatible terminal
emulator.

## Commands

```sh
cargo build            # debug build
cargo test             # unit tests (marker filter, classifier, db)
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
    proxy.rs       PTY proxy, marker dispatch, async writer
    protocol.rs    control messages (start/end)
    marker.rs      streaming private-OSC marker parser
    classifier.rs  ANSI strip + normal/empty/interactive/binary classification
  clipboard/       Clipboard trait + arboard / OSC52 / external backends
  tui/
    app.rs         App state + key handling
    ui.rs          ratatui rendering
    mod.rs         terminal setup (renders to stderr) + event loop
  commands/        one module per CLI command
shell/recall.zsh   embedded shell integrations (include_str!)
shell/recall.bash
shell/recall.fish
```

### Capture flow

1. `recall shell` -> `capture::proxy::run` opens a PTY and spawns the shell with
   `RECALL_PROXY_ACTIVE=1` and `RECALL_SESSION=<id>`.
2. The shell's preexec hook (zsh `preexec`, bash `DEBUG` trap, fish
   `fish_preexec`) writes an in-band start marker carrying the command metadata:
   `ESC ] 9999 ; {"type":"start",...} BEL`.
3. The proxy forwards PTY bytes to stdout and appends them to the active buffer.
4. The precmd hook writes the matching end marker with the exit code, before the
   prompt is drawn.
5. `marker::MarkerFilter` parses and strips both markers; `end` finalizes,
   classifies, and hands the block to a writer thread that inserts into SQLite.

There is no side channel: metadata and boundaries all travel in-band as private
OSC sequences, which keeps the tool shell- and OS-agnostic. The parser returns
marker-free chunks borrowed from the input, so the common path allocates nothing.

The TUI renders to **stderr** on purpose: stdout carries the selected command so
the zsh widget can capture it via `$(recall search --cmd-only)`. `Tab` selects
for editing (exit 0); `Ctrl+Enter` exits with code 2, which tells the widget to
execute immediately.

### Key invariants

- The end marker must be emitted in-band before the prompt, otherwise the next
  prompt is captured. `PROMPT_EOL_MARK` is blanked in proxied shells because it
  is printed before `precmd`.
- Marker payloads are JSON: escape every control character so the payload never
  contains a raw BEL/ESC that would terminate the OSC early. `print -r` keeps
  JSON escapes literal.
- zsh does not word-split unquoted expansions; pass flags as separate arguments
  or arrays.
- Shell gotchas:
  - zsh: `status` is a read-only special parameter; never assign to it.
  - bash: capture `$?` in the first `PROMPT_COMMAND` entry — later entries
    clobber it before the precmd hook runs.
  - fish: `$CMD_DURATION` is milliseconds; format durations with `math -s0` so
    the JSON has no leading zeros.
- Output is stored zstd-compressed; a truncated plain-text projection lives in
  `output_text` for FTS. Keep the two in sync.
- DB access uses WAL + `busy_timeout`; multiple processes may write.

## Conventions

- Conventional Commits, small incremental commits (`feat:`, `fix:`, `docs:`,
  `refactor:`, `test:`).
- No comments unless they explain non-obvious behavior.
- Keep `cargo fmt` and `cargo clippy --all-targets` clean.
- Prefer adding a focused unit test next to the module (`#[cfg(test)]`).
