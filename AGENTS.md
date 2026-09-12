# AGENTS.md

Guidance for agents and contributors working on `recall`.

## What this is

`recall` is a standalone, Warp-Block-style shell history and output viewer. It
runs the shell under a PTY proxy, stores command metadata and **output** in
SQLite, and presents the result in a ratatui TUI.

Target platform: Linux and macOS (zsh/bash/fish) and Windows (PowerShell 7 /
Windows PowerShell 5.1) in any VT-compatible terminal emulator. Windows uses
ConPTY through `portable-pty`.

## Commands

```sh
cargo build            # debug build
cargo test             # unit tests (marker filter, classifier, db, secrets)
cargo fmt              # formatting (run before committing)
cargo clippy --all-targets
cargo build --release
```

There is no separate lint script; use `cargo fmt` + `cargo clippy`.

### Installing a release build

The installed binary at `~/.local/bin/recall` may be in use as a running shell
proxy, so a plain `cp` fails with `ETXTBSY` ("text file busy"). Rename it first,
then replace:

```sh
cargo build --release
mv ~/.local/bin/recall ~/.local/bin/recall.old
cp target/release/recall ~/.local/bin/recall
rm -f ~/.local/bin/recall.old
```

### Testing installation changes locally

Use the developer-only harness to build the current checkout, package it like a
release, and run the production installer against that local fixture:

```sh
./scripts/dev-install.sh
```

The default mode creates an isolated home, binary directory, configuration, and
data directory under `target/dev-install.*`. It prints the command for starting
an interactive shell in that sandbox. The sandbox must not read or modify the
real user profile, Recall configuration, or history database.

Use `--user` only when a real local installation is intentional:

```sh
./scripts/dev-install.sh --user
./scripts/dev-install.sh --user --mode hooks
```

`--user` gives the production installer the real user environment, including
its normal destination selection, configuration handling, search-key choice,
and history-import prompts. `RECALL_INSTALL_DIR` and the other documented
installer environment variables continue to work. The harness supports the
Unix installer; select a shell explicitly with `--shell` when `$SHELL` is not
suitable.

Keep the production `install.sh` independent from development concerns: do not
add local-build switches to it or make it source the developer harness. The
harness intentionally invokes the unchanged production installer with a local
GitHub API, archive, and checksum substitute. This exercises the same logo,
archive extraction, checksum verification, setup, configuration, prompts, and
summary that users see without making GitHub requests. Shared post-install
behavior belongs in the `recall setup` and `recall config` Rust commands rather
than duplicated shell fragments. Live GitHub networking and published release
assets remain responsibilities of release CI.

## Architecture

```
src/
  atomic_file.rs   same-directory atomic replacement with permission retention
  main.rs          module wiring
  cli.rs           clap command definitions
  config.rs        TOML config + default paths (RECALL_CONFIG override)
  model.rs         Block + BlockKind
  util.rs          time/hostname/ANSI helpers
  db/
    schema.rs      versioned SQLite migrations + FTS5 trigram
    queries.rs     insert / import provenance / search / get / prune
  capture/
    proxy.rs       PTY proxy, marker dispatch, async writer
    protocol.rs    control messages (start/end)
    marker.rs      streaming private-OSC marker parser
    classifier.rs  ANSI strip + normal/empty/interactive/binary classification
    secrets.rs     best-effort secret detection
  clipboard/       Clipboard trait + arboard / OSC52 / external backends
  tui/
    app.rs         App state + key handling
    ui.rs          ratatui rendering
    runtime.rs     shared raw-mode / alternate-screen terminal lifecycle
    config/        interactive configuration state, effects, and rendering
    mod.rs         history TUI event loop (renders to stderr)
  commands/        one module per CLI command; optional import adapters live
    import/        below import.rs and must not leak into core models/config
shell/recall.zsh   embedded shell integrations (include_str!)
shell/recall.bash
shell/recall.fish
shell/recall.ps1
```

`util::login_shell()` resolves the shell per platform: `$SHELL` on Unix, and on
Windows the parent process chain (via `sysinfo`), then `pwsh`, `powershell`,
`%COMSPEC%`. Windows-only dependencies live under
`[target.'cfg(windows)'.dependencies]` (`sysinfo`, `windows-sys`).

### Platform boundary

Keep platform-neutral behavior out of OS-specific modules. The expected
coupling points are:

- `src/platform/{unix,windows}.rs`: PTY raw mode, login-shell arguments, input
  mode, terminal sizing, resize delivery, and pseudoconsole behavior.
- `src/util.rs`: login-shell discovery and executable lookup.
- `src/commands/shell.rs`: replacing the current process on Unix versus waiting
  for a spawned shell elsewhere.
- `src/tui/mod.rs`: terminal keyboard-protocol support.
- `src/clipboard/mod.rs`: platform clipboard backends.
- `src/shell.rs`: the table of supported shells, startup files, init scripts,
  history files, formats, and key encodings. Add shell-specific behavior to
  this table rather than branching in commands.

Changes to these boundaries require regression coverage on Linux, macOS, and
Windows. Run the full CI matrix; PTY input or teardown changes must also keep
`tests/windows_proxy.rs` passing so special-key forwarding and clean ConPTY exit
remain covered end to end.

### Windows CI and ConPTY tests

- `tests/windows_proxy.rs` drives a nested terminal: the test-side ConPTY hosts
  the recall proxy, which hosts PowerShell in a second ConPTY. Treat it as an
  asynchronous protocol, not a process that is ready immediately after spawn.
- Keep each Windows `MasterPty` alive until its child exits. Cloned reader and
  writer pipe handles do not own the pseudoconsole; dropping the master closes
  ConPTY and makes startup output depend on thread scheduling.
- Never send test input until an observable PowerShell prompt has arrived. After
  each command, wait for the next unique prompt marker before sending a special
  key or another command; seeing command output alone does not mean PSReadLine
  has resumed reading input.
- Continue answering cursor-position queries in order. Once PowerShell enables
  win32-input-mode, encode special keys as the corresponding key-down/key-up
  input records so the test exercises the same path as a terminal emulator.
- Use bounded waits that kill the child and include the captured terminal stream
  on failure. Do not stabilize this test with fixed sleeps, blind CI reruns,
  ignored failures, or by replacing it with a compile-only check.
- A non-Windows `cargo test` reports zero tests for `windows_proxy`; it is not
  Windows validation. Keep `cargo test` on all three CI platforms. Formatting
  and platform-neutral Clippy checks may run once on Linux to avoid redundant
  builds, but the Windows runtime test must remain intact.

### Capture flow

1. `recall shell` -> `capture::proxy::run` opens a PTY and spawns the shell with
   `RECALL_PROXY_ACTIVE=1` and `RECALL_SESSION=<id>`.
2. The shell's preexec hook (zsh `preexec`, bash `DEBUG` trap, fish
   `fish_preexec`, PowerShell `PSConsoleHostReadLine`) writes an in-band start
   marker carrying the command metadata:
   `ESC ] 9999 ; {"type":"start",...} BEL`.
3. The proxy forwards PTY bytes to stdout and appends them to the active buffer.
4. The precmd hook writes the matching end marker with the exit code, before the
   prompt is drawn. On PowerShell the `end` marker and a `prompt` marker are
   embedded in the string returned by the `prompt` function: Windows PowerShell
   flushes command output after calling `prompt`, so only markers written as
   part of the prompt land after the output. The `prompt` message makes the
   proxy discard the prompt text until `end`.
5. `marker::MarkerFilter` parses and strips both markers; `end` finalizes,
   classifies, and hands the block to a writer thread that inserts into SQLite.

There is no side channel: metadata and boundaries all travel in-band as private
OSC sequences, which keeps the tool shell- and OS-agnostic. The parser returns
marker-free chunks borrowed from the input, so the common path allocates nothing.

The TUI renders to **stderr** on purpose: stdout carries the selected command so
the shell widget can capture it via `recall search --cmd-only`. `Tab` selects
for editing (exit 0); `Ctrl+Enter` exits with code 2, which tells the widget to
execute immediately (`Ctrl+E` is the fallback where `Ctrl+Enter` is not
distinguishable, e.g. Windows Terminal).

### Configuration TUI

- Bare `recall config` opens the settings TUI; `path`, `show`, and `default`
  remain non-interactive. Keep `RECALL_CONFIG` resolution identical for reads,
  writes, and path display.
- `ConfigStore` reloads the active TOML before every mutation, validates the
  complete effective config, and replaces it atomically. Preserve comments,
  unrelated keys, permissions, newline style, BOM, and symlink targets. Never
  overwrite a malformed or unsupported config.
- TOML is the source of truth for UI preferences. The legacy
  `ui.list_width_pct` SQLite setting is read only for one-time migration and is
  deleted only after the TOML write succeeds.
- Configuration UI state and rendering must stay testable without a live
  terminal. Add PTY coverage for input and terminal restoration; keep a ConPTY
  smoke test for Windows.
- Shell integration actions call the same setup inspection and apply functions
  as `recall setup`. Do not duplicate profile rendering or marker handling in
  the TUI.

### Key invariants

- The end marker must be emitted in-band before the prompt, otherwise the next
  prompt is captured. `PROMPT_EOL_MARK` is blanked in proxied shells because it
  is printed before `precmd`. PowerShell has no "before prompt" hook; its end
  marker is embedded in the `prompt` return value, which the host writes after
  the command output.
- Windows (ConPTY): crossterm's raw mode does not enable VT input, so the proxy
  sets `ENABLE_VIRTUAL_TERMINAL_INPUT` itself and restores the original console
  mode on exit. Without it, special keys arrive as legacy scan codes and only
  plain letters reach the child. The pseudoconsole stays open until its master
  handle is dropped, so the proxy waits on the child in a separate thread and
  stops the resize thread to release it; otherwise the reader never sees EOF and
  the proxy hangs after `exit`. The Unix-only `-l` flag must never be passed to
  a Windows shell.
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
  - PowerShell: capture `$?` and `$LASTEXITCODE` before any statement changes
    them, then restore them before rendering the user's prompt. The integrations
    guard against double-loading and only install when PSReadLine is present.
    PSReadLine multi-stroke shortcuts use a comma-separated chord string.
- Output is stored zstd-compressed; a truncated plain-text projection lives in
  `output_text` for FTS. Keep the two in sync.
- DB access uses WAL + `busy_timeout`; multiple processes may write.

## Conventions

- Conventional Commits, small incremental commits (`feat:`, `fix:`, `docs:`,
  `refactor:`, `test:`).
- No comments unless they explain non-obvious behavior.
- Keep `cargo fmt` and `cargo clippy --all-targets` clean.
- Prefer adding a focused unit test next to the module (`#[cfg(test)]`).
