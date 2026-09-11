# recall shell integration for zsh
#
# Add to ~/.zshrc:
#     eval "$(recall init zsh)"
#
# When running under `recall shell` (RECALL_PROXY_ACTIVE=1), command metadata is
# written as private OSC markers straight into the terminal stream, where the
# proxy parses and strips them. Otherwise a metadata-only record is written in
# the background.

[[ -o interactive ]] || return 0

autoload -U add-zsh-hook
zmodload zsh/datetime 2>/dev/null

typeset -g RECALL_SESSION="${RECALL_SESSION:-$(command recall uuid 2>/dev/null)}"
typeset -g RECALL_ACTIVE_ID=""
typeset -g RECALL_LAST_START=""
typeset -g RECALL_LAST_CMD=""
typeset -g RECALL_LAST_CWD=""

# The default `%` end-of-line mark (PROMPT_SP) is emitted before precmd runs, so
# it would be captured as part of the command output. Blank it while proxied.
if [[ -n $RECALL_PROXY_ACTIVE ]]; then
  PROMPT_EOL_MARK=''
fi

_recall_json_escape() {
  local s=$1
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\n'/\\n}
  s=${s//$'\r'/\\r}
  s=${s//$'\t'/\\t}
  s=${s//$'\b'/\\b}
  s=${s//$'\f'/\\f}
  # Escape any remaining control characters so the payload stays valid JSON and
  # never contains a raw BEL/ESC that would terminate the OSC marker early.
  if [[ $s == *[[:cntrl:]]* ]]; then
    local out='' ch
    local -i i
    for (( i=1; i<=${#s}; i++ )); do
      ch=${s[i]}
      if [[ $ch == [[:cntrl:]] ]]; then
        printf -v ch '\\u%04x' "'$ch"
      fi
      out+=$ch
    done
    s=$out
  fi
  REPLY=$s
}

_recall_should_skip() {
  local cmd=$1
  [[ -z ${cmd//[[:space:]]/} ]] && return 0
  [[ $cmd =~ '^[[:space:]]*(recall|atuin)([[:space:]]|$)' ]] && return 0
  return 1
}

# Emit a private OSC marker carrying a JSON control message. `-r` keeps the
# JSON escapes literal.
_recall_emit() {
  print -rn -- $'\e]9999;'"$1"$'\a'
}

_recall_preexec() {
  local cmd=$1
  _recall_should_skip "$cmd" && return 0

  RECALL_LAST_CMD=$cmd
  RECALL_LAST_CWD=$PWD
  RECALL_LAST_START=${EPOCHREALTIME-}

  [[ -n $RECALL_PROXY_ACTIVE ]] || return 0

  RECALL_ACTIVE_ID=$(command recall uuid 2>/dev/null) || { RECALL_ACTIVE_ID=""; return 0; }

  local esc_cmd esc_cwd started_ns
  _recall_json_escape "$cmd"; esc_cmd=$REPLY
  _recall_json_escape "$PWD"; esc_cwd=$REPLY
  printf -v started_ns %.0f $(( EPOCHREALTIME * 1000000000 ))
  _recall_emit "{\"type\":\"start\",\"id\":\"$RECALL_ACTIVE_ID\",\"command\":\"$esc_cmd\",\"cwd\":\"$esc_cwd\",\"started_at\":$started_ns}"
}

_recall_precmd() {
  local exit_code=$?

  if [[ -n $RECALL_PROXY_ACTIVE && -n $RECALL_ACTIVE_ID ]]; then
    local duration_ns=""
    if [[ -n $RECALL_LAST_START ]]; then
      printf -v duration_ns %.0f $(( (EPOCHREALTIME - RECALL_LAST_START) * 1000000000 ))
    fi
    # In-band end marker: the proxy stops capturing exactly here, before the
    # next prompt is drawn.
    _recall_emit "{\"type\":\"end\",\"id\":\"$RECALL_ACTIVE_ID\",\"exit\":$exit_code${duration_ns:+,\"duration_ns\":$duration_ns}}"
    RECALL_ACTIVE_ID=""
  elif [[ -z $RECALL_PROXY_ACTIVE && -n $RECALL_LAST_CMD ]]; then
    local duration_ns=""
    if [[ -n $RECALL_LAST_START ]]; then
      printf -v duration_ns %.0f $(( (EPOCHREALTIME - RECALL_LAST_START) * 1000000000 ))
    fi
    ( command recall record --command "$RECALL_LAST_CMD" --cwd "$RECALL_LAST_CWD" --exit $exit_code --duration-ns ${duration_ns:-0} >/dev/null 2>&1 & )
  fi

  RECALL_LAST_CMD=""
  RECALL_LAST_START=""
}

add-zsh-hook preexec _recall_preexec
add-zsh-hook precmd _recall_precmd

# --- TUI widget -------------------------------------------------------------
# The TUI renders to stderr, so stdout can be captured here. The key is set by
# `ui.search_key` in the config and injected by `recall init`.
_recall_search() {
  emulate -L zsh
  zle -I

  # NB: `status` is a read-only special parameter in zsh; do not use it here.
  local recall_output recall_status
  recall_output=$(command recall search --cmd-only)
  recall_status=$?

  zle reset-prompt
  if [[ -n $recall_output ]]; then
    BUFFER=$recall_output
    CURSOR=${#BUFFER}
    if [[ $recall_status -eq 2 ]]; then
      # User pressed Ctrl+Enter in the TUI: execute immediately.
      zle accept-line
    else
      zle reset-prompt
    fi
  fi
}

if [[ -o interactive ]]; then
  zle -N _recall_search
  bindkey "@RECALL_SEARCH_KEY@" _recall_search
fi
