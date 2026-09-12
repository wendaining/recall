# Shell integration settings

The **Shell integration** category in `recall config` controls how recall is
loaded when a new interactive shell starts. It modifies the shell startup file;
it does not modify the terminal emulator's configuration.

Use `Tab` or `Shift+Tab` to select **Shell integration**, then use the up and
down arrow keys to select a row. Press `Enter` on an action to apply it.

## Status fields

The first four rows describe the detected configuration and the shell that is
currently running the settings interface.

### Detected shell

The shell for which recall is showing and editing integration settings, such as
`zsh`, `bash`, `fish`, or `pwsh`. recall normally derives this from the user's
login shell or the current platform's shell discovery rules.

### Startup file

The file that recall will update for the detected shell. Common examples are
`~/.zshrc`, `~/.bashrc`, `~/.config/fish/config.fish`, and the PowerShell
profile. The CLI command `recall setup --profile <path>` remains available for
advanced setups that use a different file.

### Current setup

The integration found in the startup file:

- **Automatic capture**: new interactive shells start through the recall PTY
  proxy and load recall's hooks. Commands, metadata, and command output are
  captured automatically.
- **Hooks only**: recall's hooks are loaded, but the shell is not automatically
  wrapped by the PTY proxy. Command metadata is recorded, while full output is
  captured only inside a shell started with `recall shell`.
- **Legacy automatic setup** or **Legacy hooks setup**: recall found an older
  installer-managed configuration. Applying either current mode migrates it to
  the current managed format.
- **Unmanaged hooks**: the startup file contains a user-written `recall init`
  command. recall recognizes and reuses it instead of adding another copy.
- **Not configured**: no recall integration was found in the startup file.
- **Malformed recall markers**: a managed block is incomplete or damaged.
  recall will not edit the file automatically because doing so could overwrite
  user configuration. Repair or remove the damaged markers manually before
  applying a mode again.
- **Unavailable** or **Error**: recall could not determine or safely inspect the
  startup file. The accompanying message describes the reason.

### Runtime

This row describes the current shell process, not merely the startup file:

- **hooks On** means recall's shell hooks are loaded in this shell.
- **proxy On** means this shell is running inside `recall shell`, so full output
  capture is active.

It is normal to see **Automatic capture** with **proxy Off** immediately after
changing the setting: the current shell started before the change. Open a new
terminal or start a new shell to activate automatic capture. Similarly,
removing integration does not unload hooks that are already running; it affects
new shells.

## Actions

### Use automatic capture

This is the recommended default. recall adds two managed blocks to the startup
file:

1. A lightweight block near the top starts `recall shell` for a supported
   interactive TTY.
2. A block near the end loads recall's hooks after the user's prompt, theme, and
   line editor configuration.

The proxied child shell skips the first block, so the user's startup file is
loaded once rather than recursively. Non-interactive shells do not start the
proxy. To bypass automatic capture for a particular new shell, set
`RECALL_PROXY=0` before starting it.

The equivalent CLI command is:

```sh
recall setup <shell> --mode auto
```

### Use hooks only

This installs the search shortcut and command metadata hooks without launching
the proxy automatically. Choose this when automatic output capture is not
wanted for every terminal. Run `recall shell` manually whenever full command
output should be captured.

The equivalent CLI command is:

```sh
recall setup <shell> --mode hooks
```

### Remove integration

This removes only blocks managed by recall and asks for confirmation first.
Other startup-file content is preserved. A user-written, unmanaged
`recall init` command is not deleted automatically.

The equivalent CLI command is:

```sh
recall setup <shell> --remove
```

## Which mode should I choose?

| Mode | Search shortcut | Command metadata | Full output by default |
| --- | --- | --- | --- |
| Automatic capture | Yes | Yes | Yes |
| Hooks only | Yes | Yes | No; run `recall shell` when needed |
| Removed / not configured | No | No | No |

Review the command and output exclusion rules in the **Capture rules** category
before enabling automatic capture for sensitive work.
