#!/bin/sh

set -eu

repo="wendaining/recall"

say() {
    printf '%s\n' "$*"
}

die() {
    printf 'recall installer: %s\n' "$*" >&2
    exit 1
}

require() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

require curl
require tar
require install
require uname
require sed
require head
require awk
require mktemp
require id
require mkdir

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Linux)
        case "$arch" in
            x86_64 | amd64) target="x86_64-unknown-linux-gnu" ;;
            aarch64 | arm64) target="aarch64-unknown-linux-gnu" ;;
            *) die "unsupported Linux architecture: $arch" ;;
        esac
        require sha256sum
        ;;
    Darwin)
        case "$arch" in
            x86_64 | amd64) target="x86_64-apple-darwin" ;;
            aarch64 | arm64) target="aarch64-apple-darwin" ;;
            *) die "unsupported macOS architecture: $arch" ;;
        esac
        require shasum
        ;;
    *) die "unsupported operating system: $os" ;;
esac

api_url="https://api.github.com/repos/$repo/releases/latest"
release_json=$(curl -fsSL --retry 3 \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "$api_url") || die "failed to query the latest GitHub release"
tag=$(printf '%s\n' "$release_json" \
    | sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1)
[ -n "$tag" ] || die "latest GitHub release did not contain a tag"

archive="recall-$tag-$target.tar.gz"
download_url="https://github.com/$repo/releases/download/$tag"
temp_dir=$(mktemp -d "${TMPDIR:-/tmp}/recall-install.XXXXXX") \
    || die "failed to create a temporary directory"

cleanup() {
    rm -rf "$temp_dir"
}
trap cleanup EXIT HUP INT TERM

say "Installing recall $tag for $target..."
curl -fL --retry 3 --progress-bar \
    -o "$temp_dir/$archive" "$download_url/$archive" \
    || die "failed to download $archive"
curl -fsSL --retry 3 \
    -o "$temp_dir/SHA256SUMS" "$download_url/SHA256SUMS" \
    || die "failed to download SHA256SUMS"

expected=$(awk -v archive="$archive" '$2 == archive { print $1; exit }' \
    "$temp_dir/SHA256SUMS")
[ -n "$expected" ] || die "SHA256SUMS does not contain $archive"

case "$os" in
    Linux) actual=$(sha256sum "$temp_dir/$archive" | awk '{print $1}') ;;
    Darwin) actual=$(shasum -a 256 "$temp_dir/$archive" | awk '{print $1}') ;;
esac
[ "$actual" = "$expected" ] || die "checksum mismatch for $archive"
say "Checksum verified."

tar -xzf "$temp_dir/$archive" -C "$temp_dir"
[ -f "$temp_dir/recall" ] || die "archive does not contain the recall binary"

if [ -n "${RECALL_INSTALL_DIR:-}" ]; then
    install_dir=$RECALL_INSTALL_DIR
elif [ "$(id -u)" -eq 0 ]; then
    install_dir=/usr/local/bin
elif [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
    install_dir=/usr/local/bin
else
    [ -n "${HOME:-}" ] || die "HOME is not set; set RECALL_INSTALL_DIR explicitly"
    install_dir="$HOME/.local/bin"
fi

mkdir -p "$install_dir"
destination="$install_dir/recall"
install -m 755 "$temp_dir/recall" "$destination"
say "Installed $("$destination" --version) to $destination"
say ""

shell_path=${SHELL:-}
case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *)
        say "Warning: $install_dir is not currently on PATH."
        case "${shell_path##*/}" in
            fish) say "Add it with: fish_add_path $install_dir" ;;
            *) say "Add this to your shell profile: export PATH=\"$install_dir:\$PATH\"" ;;
        esac
        say ""
        ;;
esac

shell_name=${shell_path##*/}
say "Next steps:"
case "$shell_name" in
    zsh)
        say '  1. Add `eval "$(recall init zsh)"` to ~/.zshrc, then restart zsh.'
        ;;
    bash)
        say '  1. Add `eval "$(recall init bash)"` to ~/.bashrc, then restart bash.'
        ;;
    fish)
        say '  1. Add `recall init fish | source` to ~/.config/fish/config.fish, then restart fish.'
        ;;
    *)
        say "  1. Enable the shell hook for zsh, bash, or fish; see the README for details."
        ;;
esac

if config_path=$("$destination" config path 2>/dev/null); then
    say "  2. Review the configuration file at: $config_path"
    if [ ! -e "$config_path" ]; then
        config_dir=$(dirname "$config_path")
        say "     Create a default file with:"
        say "       mkdir -p \"$config_dir\""
        say "       recall config default > \"$config_path\""
    fi
else
    say '  2. Run `recall config path` to find and review the configuration file.'
fi

say '  3. Run `recall` to open the history viewer; press F1 inside it for help.'
say '  4. Run `recall shell` (or configure your terminal) to capture command output.'
