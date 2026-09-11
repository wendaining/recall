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
require dirname
require grep
require cat

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

shell_path=${SHELL:-}
shell_name=${shell_path##*/}
profile_path=""
case "$shell_name" in
    zsh)
        [ -n "${HOME:-}" ] || die "HOME is not set; cannot configure zsh"
        profile_path="${ZDOTDIR:-$HOME}/.zshrc"
        ;;
    bash)
        [ -n "${HOME:-}" ] || die "HOME is not set; cannot configure bash"
        profile_path="$HOME/.bashrc"
        ;;
    fish)
        [ -n "${HOME:-}" ] || die "HOME is not set; cannot configure fish"
        profile_path="${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish"
        ;;
esac

remove_managed_setup() {
    file=$1
    [ -f "$file" ] || return 0
    sed '/^# >>> recall installer >>>$/,/^# <<< recall installer <<<$/{d;}' \
        "$file" > "$temp_dir/profile.cleaned"
    cat "$temp_dir/profile.cleaned" > "$file"
}

escape_double_quotes() {
    sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/\$/\\$/g' -e 's/`/\\`/g'
}

ask_auto_proxy() {
    case "${RECALL_AUTO_PROXY:-}" in
        1 | true | yes) return 0 ;;
        0 | false | no) return 1 ;;
        "") ;;
        *) die "RECALL_AUTO_PROXY must be 1 or 0" ;;
    esac

    if [ ! -r /dev/tty ] || [ ! -w /dev/tty ]; then
        return 1
    fi

    say ""
    say "Enable automatic output capture in every new terminal?"
    say "This makes recall's PTY proxy wrap each new interactive shell. Your usual"
    say "shell stays the same inside the proxy, while recall can record command output."
    printf 'Enable it now? [Y/n] ' > /dev/tty
    IFS= read -r answer < /dev/tty || answer=""
    case "$answer" in
        "" | y | Y | yes | YES | Yes) return 0 ;;
        *) return 1 ;;
    esac
}

auto_proxy=0
shell_configured=0
if [ -n "$profile_path" ]; then
    mkdir -p "$(dirname "$profile_path")"
    [ -e "$profile_path" ] || : > "$profile_path"
    remove_managed_setup "$profile_path"

    integration_needed=1
    case "$shell_name" in
        zsh | bash)
            if grep -Fq "recall init $shell_name" "$profile_path"; then
                integration_needed=0
            fi
            ;;
        fish)
            if grep -Fq "recall init fish" "$profile_path"; then
                integration_needed=0
            fi
            ;;
    esac

    if ask_auto_proxy; then
        auto_proxy=1
    fi

    escaped_install_dir=$(printf '%s' "$install_dir" | escape_double_quotes)
    {
        printf '\n# >>> recall installer >>>\n'
        case "$shell_name" in
            zsh | bash)
                printf 'case ":$PATH:" in\n'
                printf '  *":%s:"*) ;;\n' "$escaped_install_dir"
                printf '  *) export PATH="%s:$PATH" ;;\n' "$escaped_install_dir"
                printf 'esac\n'
                if [ "$integration_needed" -eq 1 ]; then
                    printf 'eval "$(command recall init %s)"\n' "$shell_name"
                fi
                if [ "$auto_proxy" -eq 1 ]; then
                    if [ "$shell_name" = zsh ]; then
                        printf 'if [[ -o interactive && -z ${RECALL_PROXY_ACTIVE:-} ]]; then\n'
                    else
                        printf 'if [[ $- == *i* && -z ${RECALL_PROXY_ACTIVE:-} ]]; then\n'
                    fi
                    printf '  exec recall shell\n'
                    printf 'fi\n'
                fi
                ;;
            fish)
                printf 'contains -- "%s" $PATH; or set -gx PATH "%s" $PATH\n' \
                    "$escaped_install_dir" "$escaped_install_dir"
                if [ "$integration_needed" -eq 1 ]; then
                    printf 'command recall init fish | source\n'
                fi
                if [ "$auto_proxy" -eq 1 ]; then
                    printf 'if status is-interactive; and not set -q RECALL_PROXY_ACTIVE\n'
                    printf '    exec recall shell\n'
                    printf 'end\n'
                fi
                ;;
        esac
        printf '# <<< recall installer <<<\n'
    } >> "$profile_path"
    shell_configured=1
fi

config_path=$("$destination" config path 2>/dev/null) \
    || die "installed binary could not determine the configuration path"
config_created=0
if [ ! -e "$config_path" ]; then
    config_dir=$(dirname "$config_path")
    mkdir -p "$config_dir"
    "$destination" config default > "$temp_dir/config.toml" \
        || die "failed to generate the default configuration"
    install -m 644 "$temp_dir/config.toml" "$config_path"
    config_created=1
fi

say ""
say "recall is ready."
say ""
say "Installed:     $("$destination" --version) at $destination"
if [ "$shell_configured" -eq 1 ]; then
    say "Shell setup:  $profile_path"
else
    say "Shell setup:  skipped (supported shells: zsh, bash, fish)"
fi
if [ "$config_created" -eq 1 ]; then
    say "Config:       created $config_path"
else
    say "Config:       kept existing $config_path"
fi
if [ "$auto_proxy" -eq 1 ]; then
    say "PTY proxy:    enabled for new interactive terminals"
else
    say "PTY proxy:    not enabled automatically; run 'recall shell' when needed"
fi
say ""
say "Open a new terminal to load the setup. Run 'recall' (or press Alt+R) to"
say "browse history, and press F1 inside recall to see all shortcuts."
say ""
say "Before capturing sensitive work, review: $config_path"
say "Uninstall: curl -fsSL https://raw.githubusercontent.com/$repo/master/uninstall.sh | sh"
