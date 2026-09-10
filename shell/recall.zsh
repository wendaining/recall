# recall shell integration for zsh
#
# Add to ~/.zshrc:
#     eval "$(recall init zsh)"
#
# When running under `recall shell` (RECALL_PROXY_ACTIVE=1), command metadata is
# sent to the proxy over a per-session Unix socket so output can be captured.
# Otherwise a metadata-only record is written in the background.

[[ -o interactive ]] || return 0

autoload -U add-zsh-hook
zmodload zsh/datetime 2>/dev/null
zmodload zsh/net/socket 2>/dev/null

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
  REPLY=$s
}

_recall_should_skip() {
  local cmd=$1
  [[ -z ${cmd//[[:space:]]/} ]] && return 0
  [[ $cmd =~ '^[[:space:]]*(recall|atuin)([[:space:]]|$)' ]] && return 0
  return 1
}

_recall_send() {
  [[ -n $RECALL_SOCK ]] || return 1
  [[ -S $RECALL_SOCK ]] || return 1
  zsocket "$RECALL_SOCK" 2>/dev/null || return 1
  local fd=$REPLY
  if ! print -u $fd -r -- "$1" 2>/dev/null; then
    exec {fd}>&-
    return 1
  fi
  local reply
  read -t 1 -r -u $fd reply 2>/dev/null
  exec {fd}>&-
  return 0
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
  local payload="{\"type\":\"start\",\"id\":\"$RECALL_ACTIVE_ID\",\"command\":\"$esc_cmd\",\"cwd\":\"$esc_cwd\",\"started_at\":$started_ns}"
  _recall_send "$payload" || RECALL_ACTIVE_ID=""
}

_recall_precmd() {
  local exit_code=$?

  if [[ -n $RECALL_PROXY_ACTIVE && -n $RECALL_ACTIVE_ID ]]; then
    local duration_ns=""
    if [[ -n $RECALL_LAST_START ]]; then
      printf -v duration_ns %.0f $(( (EPOCHREALTIME - RECALL_LAST_START) * 1000000000 ))
    fi
    local payload="{\"type\":\"end\",\"id\":\"$RECALL_ACTIVE_ID\",\"exit\":$exit_code${duration_ns:+,\"duration_ns\":$duration_ns}}"
    # In-band marker so the proxy stops capturing at the exact boundary and
    # does not attribute the next prompt to this command.
    print -n -- $'\e]9999;recall-end\a'
    _recall_send "$payload"
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
