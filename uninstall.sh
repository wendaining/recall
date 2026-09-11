#!/bin/sh

set -eu

say() {
    printf '%s\n' "$*"
}

remove_managed_setup() {
    file=$1
    [ -f "$file" ] || return 0
    grep -Fq '# >>> recall installer >>>' "$file" || return 0

    temp_file=$(mktemp "${TMPDIR:-/tmp}/recall-uninstall.XXXXXX") \
        || return 1
    sed '/^# >>> recall installer >>>$/,/^# <<< recall installer <<<$/{d;}' \
        "$file" > "$temp_file"
    cat "$temp_file" > "$file"
    rm -f "$temp_file"
    say "Removed installer-managed setup from $file"
}

remove_binary() {
    candidate=$1
    if [ -f "$candidate" ] || [ -L "$candidate" ]; then
        binary_found=1
        if [ -w "$candidate" ] || [ -w "$(dirname "$candidate")" ]; then
            rm -f "$candidate"
            say "Removed $candidate"
            removed_binary=1
        else
            say "Could not remove $candidate (permission denied)."
            say "Remove it with an account that can write to that directory."
        fi
    fi
}

for command_name in sed cat rm mktemp dirname grep; do
    command -v "$command_name" >/dev/null 2>&1 || {
        printf 'recall uninstaller: required command not found: %s\n' "$command_name" >&2
        exit 1
    }
done

config_path=""
if command -v recall >/dev/null 2>&1; then
    config_path=$(recall config path 2>/dev/null || true)
fi

if [ -n "${HOME:-}" ]; then
    remove_managed_setup "${ZDOTDIR:-$HOME}/.zshrc"
    remove_managed_setup "$HOME/.bashrc"
    remove_managed_setup "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish"
fi

binary_found=0
removed_binary=0
if [ -n "${RECALL_INSTALL_DIR:-}" ]; then
    remove_binary "$RECALL_INSTALL_DIR/recall"
else
    remove_binary "/usr/local/bin/recall"
    if [ -n "${HOME:-}" ]; then
        remove_binary "$HOME/.local/bin/recall"
    fi
fi

if [ "$binary_found" -eq 0 ]; then
    say "No recall binary was found in the installer locations."
fi

say ""
if [ "$binary_found" -eq 1 ] && [ "$removed_binary" -eq 0 ]; then
    say "The shell setup was removed, but the recall binary is still installed."
else
    say "recall has been uninstalled from your shell setup."
fi
say "Your configuration and command history were kept."
if [ -n "$config_path" ]; then
    say "Configuration: $config_path"
fi
say "You can delete the configuration and data manually if you no longer need them."

if [ "$binary_found" -eq 1 ] && [ "$removed_binary" -eq 0 ]; then
    exit 1
fi
