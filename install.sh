#!/bin/sh

set -eu

repo="wendaining/recall"

style_reset=""
style_bold=""
style_dim=""
style_cyan=""
style_green=""
style_yellow=""
style_error_red=""
style_error_reset=""
escape=$(printf '\033')
if [ -t 1 ] \
    && [ "${TERM:-dumb}" != dumb ] \
    && [ -z "${NO_COLOR:-}" ] \
    && [ "${CLICOLOR:-1}" != 0 ]; then
    style_reset="${escape}[0m"
    style_bold="${escape}[1m"
    style_dim="${escape}[2m"
    style_cyan="${escape}[36m"
    style_green="${escape}[32m"
    style_yellow="${escape}[33m"
fi
if [ -t 2 ] \
    && [ "${TERM:-dumb}" != dumb ] \
    && [ -z "${NO_COLOR:-}" ] \
    && [ "${CLICOLOR:-1}" != 0 ]; then
    style_error_red="${escape}[31m"
    style_error_reset="${escape}[0m"
fi

say() {
    printf '%s\n' "$*"
}

step() {
    printf '%s›%s %s%s%s\n' \
        "$style_cyan" "$style_reset" "$style_bold" "$*" "$style_reset"
}

success() {
    printf '%s✓%s %s\n' "$style_green" "$style_reset" "$*"
}

warning() {
    printf '%s!%s %s\n' "$style_yellow" "$style_reset" "$*"
}

summary() {
    summary_label=$1
    shift
    printf '  %s%-13s%s %s\n' "$style_cyan" "$summary_label" "$style_reset" "$*"
}

muted() {
    printf '%s%s%s\n' "$style_dim" "$*" "$style_reset"
}

die() {
    printf '%s✗%s recall installer: %s\n' \
        "$style_error_red" "$style_error_reset" "$*" >&2
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
require cat

show_logo() {
    printf '%s' "$style_cyan"
    cat <<'EOF'
             .-=================-.
          .-'                     `-.
        .'       +----------+        `.
       /        /    >_      \         \
      ;        |      _       |         ;
      |        |              |         |
      ;        |              |         ;
       \        \            /         /
        `.       +----------+        .'
          `-.                     _.-'
             `-=================-'
EOF
    printf '%s' "$style_reset"
}

show_logo

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

step "Installing recall $tag for $target..."
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
success "Checksum verified."

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

set_search_key() {
    file=$1
    key=$2
    awk -v key="$key" '
        BEGIN { done = 0; in_ui = 0; seen_ui = 0 }
        /^\[ui\][[:space:]]*$/ {
            in_ui = 1; seen_ui = 1
            print
            next
        }
        /^\[/ {
            if (in_ui && !done) { print "search_key = \"" key "\""; done = 1 }
            in_ui = 0
        }
        in_ui && /^[[:space:]]*search_key[[:space:]]*=/ {
            if (!done) { print "search_key = \"" key "\""; done = 1 }
            next
        }
        { print }
        END {
            if (in_ui && !done) { print "search_key = \"" key "\""; done = 1 }
            if (!seen_ui) { print ""; print "[ui]"; print "search_key = \"" key "\"" }
        }
    ' "$file" > "$temp_dir/config.new" \
        || die "failed to update the search key in $file"
    cat "$temp_dir/config.new" > "$file"
}

choose_proxy_setup() {
    case "${RECALL_PROXY_SETUP:-}" in
        "" | auto | shell) proxy_setup=auto ;;
        hooks | none) proxy_setup=hooks ;;
        terminal) proxy_setup=terminal ;;
        *) die "RECALL_PROXY_SETUP must be auto, hooks, or terminal" ;;
    esac
}

choose_search_key() {
    search_key=""
    [ "$os" = Darwin ] || return 0

    case "${RECALL_SEARCH_KEY:-}" in
        "" ) ;;
        default | alt-r ) return 0 ;;
        * ) search_key=$RECALL_SEARCH_KEY; return 0 ;;
    esac

    if [ ! -r /dev/tty ] || [ ! -w /dev/tty ]; then
        return 0
    fi

    say ""
    say "On macOS, Option+R only works after enabling \"Use Option as Meta key\""
    say "in your terminal. Choose the key that opens recall:"
    say "  1. Ctrl+X Ctrl+R (recommended, rarely used)"
    say "  2. Ctrl+T"
    say "  3. Keep the default Alt+R"
    printf 'Choice [1]: ' > /dev/tty
    IFS= read -r answer < /dev/tty || answer=""
    case "$answer" in
        2) search_key="ctrl-t" ;;
        3) search_key="" ;;
        *) search_key="ctrl-x ctrl-r" ;;
    esac
}

offer_history_import() {
    import_kind=$1
    import_shell=$2
    import_path=$3
    import_label=$4
    [ -f "$import_path" ] || return 0

    case "${RECALL_IMPORT_HISTORY:-ask}" in
        1 | true | yes) import_answer=yes ;;
        0 | false | no) return 0 ;;
        ask | "")
            if [ ! -r /dev/tty ] || [ ! -w /dev/tty ]; then
                return 0
            fi
            say ""
            printf 'Import existing %s from %s? [Y/n] ' \
                "$import_label" "$import_path" > /dev/tty
            IFS= read -r import_answer < /dev/tty || import_answer=no
            ;;
        *) die "RECALL_IMPORT_HISTORY must be yes, no, or ask" ;;
    esac

    case "$import_answer" in
        "" | y | Y | yes | YES | Yes) ;;
        *) return 0 ;;
    esac

    if [ "$import_kind" = atuin ]; then
        if "$destination" import atuin --path "$import_path"; then
            say "Imported $import_label."
        else
            warning "Could not import $import_label; installation will continue."
        fi
    elif "$destination" import history "$import_shell" --path "$import_path"; then
        say "Imported $import_label."
    else
        warning "Could not import $import_label; installation will continue."
    fi
}

offer_detected_history() {
    [ -n "${HOME:-}" ] || return 0

    case "${XDG_DATA_HOME:-}" in
        /*) history_data_dir=$XDG_DATA_HOME ;;
        *) history_data_dir="$HOME/.local/share" ;;
    esac
    offer_history_import atuin "" \
        "$history_data_dir/atuin/history.db" "atuin history"

    offer_history_import history zsh \
        "${ZDOTDIR:-$HOME}/.zsh_history" "zsh history"
    offer_history_import history bash \
        "$HOME/.bash_history" "bash history"
    offer_history_import history fish \
        "$history_data_dir/fish/fish_history" "fish history"
}

proxy_setup=auto
choose_proxy_setup
search_key=""
choose_search_key
shell_configured=0
if [ -n "$profile_path" ]; then
    setup_mode=$proxy_setup
    [ "$setup_mode" = terminal ] && setup_mode=hooks
    "$destination" setup "$shell_name" --mode "$setup_mode" --profile "$profile_path" \
        || die "failed to configure $profile_path"
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

if [ -n "$search_key" ]; then
    set_search_key "$config_path" "$search_key"
fi

offer_detected_history

say ""
success "recall is ready."
say ""
summary "Installed:" "$("$destination" --version) at $destination"
if [ "$shell_configured" -eq 1 ]; then
    summary "Shell setup:" "$profile_path"
else
    summary "Shell setup:" "skipped (supported shells: zsh, bash, fish)"
fi
if [ "$config_created" -eq 1 ]; then
    summary "Config:" "created $config_path"
else
    summary "Config:" "kept existing $config_path"
fi
if [ "$shell_configured" -eq 0 ]; then
    summary "Output:" "not configured; run 'recall shell' manually"
else
    case "$proxy_setup" in
        auto)
            summary "Output:" "automatic capture in new interactive shells"
            summary "Alternative:" "run 'recall setup $shell_name --mode hooks' for hooks only"
            ;;
        hooks)
            summary "Output:" "hooks only; run 'recall shell' when capture is needed"
            ;;
        terminal)
            summary "Output:" "terminal-managed compatibility mode"
            say "              set your terminal's startup command to:"
            say "              $destination shell"
            ;;
    esac
fi
say ""
step "Open a new terminal to activate the setup."
say "Run 'recall' or press"
say "Alt+R to browse history; press F1 inside recall to see all shortcuts."
if [ "$os" = Darwin ] && [ -z "$search_key" ]; then
    say ""
    warning "macOS: if Option+R types '®' instead of opening recall, enable"
    say "\"Use Option as Meta key\" in your terminal (Terminal.app, iTerm2, Ghostty)."
fi
summary "Search key:" "${search_key:-alt-r} (edit [ui].search_key in the config to change it)"
say ""
warning "Before capturing sensitive work, review: $config_path"
muted "Uninstall: curl -fsSL https://raw.githubusercontent.com/$repo/master/uninstall.sh | sh"
