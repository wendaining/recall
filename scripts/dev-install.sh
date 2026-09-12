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
Run the production installer with a release built from the current checkout.

Usage: scripts/dev-install.sh [--user] [--mode auto|hooks] [--shell SHELL]

Options:
  --user         Use the real user environment instead of an isolated sandbox.
  --mode MODE    Pass auto or hooks to the production installer.
  --shell SHELL  Test with a specific zsh, bash, or fish executable.
  -h, --help     Show this help.

The script builds a local release archive, substitutes it for GitHub downloads,
and then runs install.sh unchanged. By default, all installed files and shell
profiles live under target/dev-install.*.
EOF
}

user_install=0
setup_mode=${RECALL_PROXY_SETUP:-auto}
shell_path=${SHELL:-}

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
            [ "$#" -ge 2 ] || die "--shell requires an executable"
            shell_path=$2
            shift
            ;;
        --shell=*)
            shell_path=${1#*=}
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
    auto | hooks | shell | none | terminal) ;;
    *) die "--mode must be auto, hooks, or a supported compatibility alias" ;;
esac

if [ -n "$shell_path" ]; then
    case "$shell_path" in
        */*) [ -x "$shell_path" ] || die "shell is not executable: $shell_path" ;;
        *)
            resolved_shell=$(command -v "$shell_path" || true)
            [ -n "$resolved_shell" ] || die "shell not found: $shell_path"
            shell_path=$resolved_shell
            ;;
    esac
fi

require cargo
require git
require install
require tar
require uname
require mktemp
require mkdir
require dirname
require cp
require chmod

script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
repo_root=$(dirname "$script_dir")
[ -f "$repo_root/Cargo.toml" ] || die "run this script from a recall checkout"
[ -f "$repo_root/install.sh" ] || die "production installer not found"

os=$(uname -s)
arch=$(uname -m)
case "$os" in
    Linux)
        case "$arch" in
            x86_64 | amd64) target=x86_64-unknown-linux-gnu ;;
            aarch64 | arm64) target=aarch64-unknown-linux-gnu ;;
            *) die "unsupported Linux architecture: $arch" ;;
        esac
        require sha256sum
        ;;
    Darwin)
        case "$arch" in
            x86_64 | amd64) target=x86_64-apple-darwin ;;
            aarch64 | arm64) target=aarch64-apple-darwin ;;
            *) die "unsupported macOS architecture: $arch" ;;
        esac
        require shasum
        ;;
    *) die "unsupported operating system: $os" ;;
esac

target_dir="$repo_root/target"
say "Building the current checkout in release mode..."
CARGO_TARGET_DIR="$target_dir" cargo build \
    --release --locked --manifest-path "$repo_root/Cargo.toml"

built_binary="$target_dir/release/recall"
[ -f "$built_binary" ] || die "release build did not produce $built_binary"

revision=$(git -C "$repo_root" rev-parse --short HEAD)
if [ -n "$(git -C "$repo_root" status --porcelain)" ]; then
    revision="$revision-dirty"
fi
tag="local-$revision"
archive_name="recall-$tag-$target.tar.gz"

mkdir -p "$target_dir"
fixture_dir=$(mktemp -d "$target_dir/dev-release.XXXXXX") \
    || die "failed to create a local release fixture"
cleanup() {
    rm -rf "$fixture_dir"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$fixture_dir/payload" "$fixture_dir/bin"
install -m 755 "$built_binary" "$fixture_dir/payload/recall"
tar -czf "$fixture_dir/$archive_name" -C "$fixture_dir/payload" recall
case "$os" in
    Linux)
        (cd "$fixture_dir" && sha256sum "$archive_name" > SHA256SUMS)
        ;;
    Darwin)
        (cd "$fixture_dir" && shasum -a 256 "$archive_name" > SHA256SUMS)
        ;;
esac

cat > "$fixture_dir/bin/curl" <<'EOF'
#!/bin/sh

set -eu

output=""
url=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o | --output)
            [ "$#" -ge 2 ] || exit 2
            output=$2
            shift
            ;;
        -H | --header | --retry)
            [ "$#" -ge 2 ] || exit 2
            shift
            ;;
        -*) ;;
        *) url=$1 ;;
    esac
    shift
done

case "$url" in
    https://api.github.com/repos/wendaining/recall/releases/latest)
        printf '{\n  "tag_name": "%s"\n}\n' "$RECALL_DEV_TAG"
        ;;
    */"$RECALL_DEV_ARCHIVE_NAME")
        [ -n "$output" ] || exit 2
        cp "$RECALL_DEV_ARCHIVE" "$output"
        ;;
    */SHA256SUMS)
        [ -n "$output" ] || exit 2
        cp "$RECALL_DEV_SUMS" "$output"
        ;;
    *)
        printf 'recall dev installer: unexpected download URL: %s\n' "$url" >&2
        exit 2
        ;;
esac
EOF
chmod 755 "$fixture_dir/bin/curl"

export RECALL_DEV_TAG=$tag
export RECALL_DEV_ARCHIVE_NAME=$archive_name
export RECALL_DEV_ARCHIVE="$fixture_dir/$archive_name"
export RECALL_DEV_SUMS="$fixture_dir/SHA256SUMS"
export RECALL_PROXY_SETUP=$setup_mode
export SHELL=$shell_path

if [ "$user_install" -eq 1 ]; then
    PATH="$fixture_dir/bin:${PATH:-}" sh "$repo_root/install.sh"
else
    sandbox_root=$(mktemp -d "$target_dir/dev-install.XXXXXX") \
        || die "failed to create an installation sandbox"
    install_home="$sandbox_root/home"
    install_dir="$sandbox_root/bin"
    config_home="$install_home/.config"
    data_home="$install_home/.local/share"
    recall_config="$config_home/recall/config.toml"
    mkdir -p "$install_home"

    HOME="$install_home" \
        ZDOTDIR="$install_home" \
        XDG_CONFIG_HOME="$config_home" \
        XDG_DATA_HOME="$data_home" \
        RECALL_CONFIG="$recall_config" \
        RECALL_INSTALL_DIR="$install_dir" \
        PATH="$fixture_dir/bin:${PATH:-}" \
        sh "$repo_root/install.sh"

    case "${TERM:-}" in
        "" | dumb) test_term=xterm-256color ;;
        *) test_term=$TERM ;;
    esac
    say ""
    say "Development source: local checkout $revision (no GitHub requests)"
    say "Sandbox:           $sandbox_root"
    if [ -n "$shell_path" ]; then
        say ""
        say "Start an isolated shell with:"
        say "  env HOME=\"$install_home\" ZDOTDIR=\"$install_home\" XDG_CONFIG_HOME=\"$config_home\" XDG_DATA_HOME=\"$data_home\" RECALL_CONFIG=\"$recall_config\" PATH=\"$install_dir:\$PATH\" SHELL=\"$shell_path\" TERM=\"$test_term\" \"$shell_path\" -i"
    fi
    say ""
    say "Remove the sandbox when finished:"
    say "  rm -rf \"$sandbox_root\""
fi
