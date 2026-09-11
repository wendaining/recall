# recall shell integration for fish
#
# Add to ~/.config/fish/config.fish:
#     recall init fish | source
#
# When running under `recall shell` (RECALL_PROXY_ACTIVE=1), command metadata is
# written as private OSC markers into the terminal stream, where the proxy
# parses and strips them. Otherwise a metadata-only record is written.

status is-interactive; or exit 0

if set -q __RECALL_INSTALLED
    exit 0
end
set -g __RECALL_INSTALLED 1

if not set -q RECALL_SESSION
    set -gx RECALL_SESSION (command recall uuid 2>/dev/null)
end
set -gx RECALL_SHELL fish

set -g RECALL_ACTIVE_ID ""
set -g RECALL_LAST_CMD ""
set -g RECALL_LAST_CWD ""
set -g RECALL_LAST_START ""

function __recall_now_ns
    printf '%s000000000' (date +%s)
end

function __recall_json_escape
    set -l s "$argv[1]"
    set -l out
    for c in (string split '' -- "$s")
        switch $c
            case '\\'
                set out $out '\\\\'
            case '"'
                set out $out '\\"'
            case '*'
                if string match -qr '[[:cntrl:]]' -- "$c"
                    set -l code (printf '%d' "'$c")
                    set out $out (printf '%s%04x' '\u' $code)
                else
                    set out $out "$c"
                end
        end
    end
    string join '' -- $out
end

function __recall_should_skip
    set -l cmd "$argv[1]"
    test -z (string trim -- "$cmd"); and return 0
    string match -qr '^\s*recall(\s|$)' -- "$cmd"; and return 0
    return 1
end

function __recall_emit
    printf '\033]9999;%s\007' "$argv[1]"
end

function __recall_preexec --on-event fish_preexec
    set -l cmd "$argv[1]"
    __recall_should_skip "$cmd"; and return

    set -g RECALL_LAST_CMD "$cmd"
    set -g RECALL_LAST_CWD "$PWD"
    set -g RECALL_LAST_START (__recall_now_ns)

    set -q RECALL_PROXY_ACTIVE; or return

    set -g RECALL_ACTIVE_ID (command recall uuid 2>/dev/null)
    test -n "$RECALL_ACTIVE_ID"; or return

    set -l esc_cmd (__recall_json_escape "$cmd")
    set -l esc_cwd (__recall_json_escape "$PWD")
    __recall_emit "{\"type\":\"start\",\"id\":\"$RECALL_ACTIVE_ID\",\"command\":\"$esc_cmd\",\"cwd\":\"$esc_cwd\",\"started_at\":$RECALL_LAST_START}"
end

function __recall_postexec --on-event fish_postexec
    set -l exit_code $status

    if set -q RECALL_PROXY_ACTIVE; and test -n "$RECALL_ACTIVE_ID"
        set -l duration_ns ""
        if test -n "$CMD_DURATION"
            set duration_ns (math -s0 "$CMD_DURATION * 1000000")
        end
        set -l dur ""
        test -n "$duration_ns"; and set dur ",\"duration_ns\":$duration_ns"
        __recall_emit "{\"type\":\"end\",\"id\":\"$RECALL_ACTIVE_ID\",\"exit\":$exit_code$dur}"
        set -g RECALL_ACTIVE_ID ""
    else if not set -q RECALL_PROXY_ACTIVE; and test -n "$RECALL_LAST_CMD"
        set -l duration_ns 0
        if test -n "$CMD_DURATION"
            set duration_ns (math -s0 "$CMD_DURATION * 1000000")
        end
        command recall record --command "$RECALL_LAST_CMD" --cwd "$RECALL_LAST_CWD" --exit "$exit_code" --duration-ns "$duration_ns" --shell fish >/dev/null 2>&1 &
    end

    set -g RECALL_LAST_CMD ""
    set -g RECALL_LAST_START ""
end

# --- TUI widget -------------------------------------------------------------
# The TUI renders to stderr, so stdout can be captured here. The key is set by
# `ui.search_key` in the config and injected by `recall init`.
function __recall_search
    set -l output (command recall search --cmd-only 2>/dev/tty | string collect)
    set -l rc $status
    if test -n "$output"
        commandline -r -- "$output"
        if test $rc -eq 2
            commandline -f execute
        else
            commandline -f repaint
        end
    end
end

bind @RECALL_SEARCH_KEY@ __recall_search
