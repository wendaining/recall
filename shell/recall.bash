# recall shell integration for bash
#
# Add to ~/.bashrc:
#     eval "$(recall init bash)"
#
# When running under `recall shell` (RECALL_PROXY_ACTIVE=1), command metadata is
# written as private OSC markers into the terminal stream, where the proxy
# parses and strips them. Otherwise a metadata-only record is written.

[[ $- == *i* ]] || return 0

# Install only once.
[[ -n ${__RECALL_INSTALLED:-} ]] && return 0
__RECALL_INSTALLED=1

RECALL_SESSION="${RECALL_SESSION:-$(command recall uuid 2>/dev/null)}"
export RECALL_SESSION
export RECALL_SHELL=bash

RECALL_ACTIVE_ID=""
RECALL_LAST_CMD=""
RECALL_LAST_CWD=""
RECALL_LAST_START=""
__RECALL_IN_PROMPT=1
__RECALL_PREEXEC_DONE=0

__recall_now_ns() {
  if [[ -n ${EPOCHREALTIME:-} ]]; then
    local s=${EPOCHREALTIME%.*} frac=${EPOCHREALTIME#*.}
    printf '%s%s000' "$s" "${frac:0:6}"
  else
    date +%s%N
  fi
}

__recall_json_escape() {
  local s=$1
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\n'/\\n}
  s=${s//$'\r'/\\r}
  s=${s//$'\t'/\\t}
  s=${s//$'\b'/\\b}
  s=${s//$'\f'/\\f}
  if [[ $s == *[[:cntrl:]]* ]]; then
    local out='' ch i
    for (( i=0; i<${#s}; i++ )); do
      ch=${s:i:1}
      if [[ $ch == [[:cntrl:]] ]]; then
        printf -v ch '\\u%04x' "'$ch"
      fi
      out+=$ch
    done
    s=$out
  fi
  printf '%s' "$s"
}

__recall_should_skip() {
  local cmd=$1
  [[ -z ${cmd//[[:space:]]/} ]] && return 0
  [[ $cmd =~ ^[[:space:]]*recall([[:space:]]|$) ]] && return 0
  return 1
}

__recall_emit() {
  printf '\033]9999;%s\007' "$1"
}

__recall_preexec() {
  local cmd=$1
  __recall_should_skip "$cmd" && return 0

  RECALL_LAST_CMD=$cmd
  RECALL_LAST_CWD=$PWD
  RECALL_LAST_START=$(__recall_now_ns)

  [[ -n ${RECALL_PROXY_ACTIVE:-} ]] || return 0

  RECALL_ACTIVE_ID=$(command recall uuid 2>/dev/null) || { RECALL_ACTIVE_ID=""; return 0; }

  local esc_cmd esc_cwd
  esc_cmd=$(__recall_json_escape "$cmd")
  esc_cwd=$(__recall_json_escape "$PWD")
  __recall_emit "{\"type\":\"start\",\"id\":\"$RECALL_ACTIVE_ID\",\"command\":\"$esc_cmd\",\"cwd\":\"$esc_cwd\",\"started_at\":$RECALL_LAST_START}"
}

__recall_precmd() {
  local exit_code=${__RECALL_LAST_STATUS:-0}

  if [[ -n ${RECALL_PROXY_ACTIVE:-} && -n $RECALL_ACTIVE_ID ]]; then
    local duration_ns=""
    if [[ -n $RECALL_LAST_START ]]; then
      duration_ns=$(( $(__recall_now_ns) - RECALL_LAST_START ))
    fi
    __recall_emit "{\"type\":\"end\",\"id\":\"$RECALL_ACTIVE_ID\",\"exit\":$exit_code${duration_ns:+,\"duration_ns\":$duration_ns}}"
    RECALL_ACTIVE_ID=""
  elif [[ -z ${RECALL_PROXY_ACTIVE:-} && -n $RECALL_LAST_CMD ]]; then
    local duration_ns=0
    if [[ -n $RECALL_LAST_START ]]; then
      duration_ns=$(( $(__recall_now_ns) - RECALL_LAST_START ))
    fi
    ( command recall record --command "$RECALL_LAST_CMD" --cwd "$RECALL_LAST_CWD" --exit "$exit_code" --duration-ns "$duration_ns" --shell bash >/dev/null 2>&1 & )
  fi

  RECALL_LAST_CMD=""
  RECALL_LAST_START=""
  __RECALL_PREEXEC_DONE=0
  __RECALL_IN_PROMPT=0
}

__recall_debug_trap() {
  [[ $__RECALL_IN_PROMPT == 1 ]] && return
  [[ $__RECALL_PREEXEC_DONE == 1 ]] && return
  case $BASH_COMMAND in
    __recall_*) return ;;
  esac
  __RECALL_PREEXEC_DONE=1
  __recall_preexec "$BASH_COMMAND"
}

__recall_prompt_begin() {
  __RECALL_LAST_STATUS=$?
  __RECALL_IN_PROMPT=1
}

trap '__recall_debug_trap' DEBUG
PROMPT_COMMAND="__recall_prompt_begin${PROMPT_COMMAND:+; $PROMPT_COMMAND}; __recall_precmd"

# --- TUI widget -------------------------------------------------------------
# The TUI renders to stderr, so stdout can be captured here. The key is set by
# `ui.search_key` in the config and injected by `recall init`.
# bash cannot accept a line from a bind -x function, so both Tab and Ctrl+Enter
# insert the command; press Enter to run it.
__recall_search() {
  local output
  if [[ $OSTYPE == darwin* ]]; then
    output=$(command recall search --cmd-only </dev/tty 2>/dev/tty)
  else
    output=$(command recall search --cmd-only 2>/dev/tty)
  fi
  if [[ -n $output ]]; then
    READLINE_LINE=$output
    READLINE_POINT=${#READLINE_LINE}
  fi
}

bind -x '"@RECALL_SEARCH_KEY@": __recall_search'
if [[ $OSTYPE == darwin* ]]; then
  bind -x '"®": __recall_search'
fi
