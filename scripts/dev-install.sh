#!/bin/sh

set -eu

say() {
    printf '%s\n' "$*"
}

die() {
    printf 'recall dev installer: %s\n' "$*" >&2
    exit 1
}

require() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

usage() {
    cat <<'EOF'
Build and install the current recall checkout for local testing.

Usage: scripts/dev-install.sh [--user] [--mode auto|hooks] [--shell SHELL]

Options:
  --user         Install into the real user environment instead of a sandbox.
  --mode MODE    Configure automatic capture (auto) or hooks only (hooks).
  --shell SHELL  Configure zsh, bash, or fish; defaults to $SHELL.
  -h, --help     Show this help.

The default sandbox is created under target/dev-install.* and does not modify
the real user profile, configuration, or history database.
EOF
}

user_install=0
setup_mode=auto
shell_path=${SHELL:-}
shell_name=${shell_path##*/}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --user)
            user_install=1
            ;;
        --mode)
            [ "$#" -ge 2 ] || die "--mode requires auto or hooks"
            setup_mode=$2
            shift
            ;;
        --mode=*)
            setup_mode=${1#*=}
            ;;
        --shell)
            [ "$#" -ge 2 ] || die "--shell requires zsh, bash, or fish"
            shell_name=${2##*/}
            shift
            ;;
        --shell=*)
            shell_name=${1#*=}
            shell_name=${shell_name##*/}
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            die "unknown option: $1"
            ;;
    esac
    shift
done

case "$setup_mode" in
    auto | hooks) ;;
    *) die "--mode must be auto or hooks" ;;
esac

case "$shell_name" in
    zsh | bash | fish) ;;
    "") die "SHELL is not set; pass --shell zsh, bash, or fish" ;;
    *) die "unsupported shell: $shell_name" ;;
esac

require cargo
require git
require install
require mkdir
require mktemp
require mv
require dirname

script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
repo_root=$(dirname "$script_dir")
[ -f "$repo_root/Cargo.toml" ] || die "run this script from a recall checkout"

target_dir="$repo_root/target"
say "Building the current checkout in release mode..."
CARGO_TARGET_DIR="$target_dir" cargo build \
    --release --locked --manifest-path "$repo_root/Cargo.toml"

built_binary="$target_dir/release/recall"
[ -f "$built_binary" ] || die "release build did not produce $built_binary"

if [ "$user_install" -eq 1 ]; then
    [ -n "${HOME:-}" ] || die "HOME is not set"
    install_home=$HOME
    install_dir=${RECALL_INSTALL_DIR:-$HOME/.local/bin}
    config_home=${XDG_CONFIG_HOME:-$HOME/.config}
    data_home=${XDG_DATA_HOME:-$HOME/.local/share}
    recall_config=${RECALL_CONFIG:-$config_home/recall/config.toml}
    zdotdir=${ZDOTDIR:-$HOME}
    sandbox_root=""
else
    mkdir -p "$target_dir"
    sandbox_root=$(mktemp -d "$target_dir/dev-install.XXXXXX") \
        || die "failed to create a sandbox"
    install_home="$sandbox_root/home"
    install_dir="$sandbox_root/bin"
    config_home="$install_home/.config"
    data_home="$install_home/.local/share"
    recall_config="$config_home/recall/config.toml"
    zdotdir=$install_home
fi

mkdir -p "$install_home" "$install_dir" "$config_home" "$data_home"
destination="$install_dir/recall"
staged_binary="$install_dir/.recall.dev-install.$$"
install -m 755 "$built_binary" "$staged_binary"
mv -f "$staged_binary" "$destination"

run_recall() {
    HOME="$install_home" \
        ZDOTDIR="$zdotdir" \
        XDG_CONFIG_HOME="$config_home" \
        XDG_DATA_HOME="$data_home" \
        RECALL_CONFIG="$recall_config" \
        PATH="$install_dir:${PATH:-}" \
        "$destination" "$@"
}

run_recall setup "$shell_name" --mode "$setup_mode"

config_path=$(run_recall config path)
config_created=0
if [ ! -e "$config_path" ]; then
    config_dir=$(dirname "$config_path")
    mkdir -p "$config_dir"
    config_tmp="$config_dir/.config.toml.dev-install.$$"
    run_recall config default > "$config_tmp"
    install -m 644 "$config_tmp" "$config_path"
    rm -f "$config_tmp"
    config_created=1
fi

revision=$(git -C "$repo_root" rev-parse --short HEAD)
if [ -n "$(git -C "$repo_root" status --porcelain)" ]; then
    revision="$revision-dirty"
fi

say ""
say "recall development build is ready."
say ""
say "Source:       local checkout $revision"
say "Installed:    $(run_recall --version) at $destination"
say "Shell setup:  $shell_name ($setup_mode)"
if [ "$config_created" -eq 1 ]; then
    say "Config:       created $config_path"
else
    say "Config:       kept existing $config_path"
fi
say "History:      not imported"

if [ "$user_install" -eq 1 ]; then
    say ""
    say "Open a new terminal to try the development build."
    say "Review $config_path before capturing sensitive commands."
else
    shell_executable=$(command -v "$shell_name" || true)
    [ -n "$shell_executable" ] || shell_executable=$shell_name
    case "${TERM:-}" in
        "" | dumb) test_term=xterm-256color ;;
        *) test_term=$TERM ;;
    esac
    say "Sandbox:      $sandbox_root"
    say ""
    say "Start an isolated shell with:"
    say "  env HOME=\"$install_home\" ZDOTDIR=\"$zdotdir\" XDG_CONFIG_HOME=\"$config_home\" XDG_DATA_HOME=\"$data_home\" RECALL_CONFIG=\"$recall_config\" PATH=\"$install_dir:\$PATH\" SHELL=\"$shell_executable\" TERM=\"$test_term\" \"$shell_executable\" -i"
    say ""
    say "Remove the sandbox when finished:"
    say "  rm -rf \"$sandbox_root\""
fi
