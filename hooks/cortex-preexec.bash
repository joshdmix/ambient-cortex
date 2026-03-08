#!/usr/bin/env bash
# Ambient Cortex — bash shell integration
# Source this file in your .bashrc:
#   source ~/.config/cortex/cortex-preexec.bash

__cortex_preexec() {
    __cortex_cmd="$1"
    __cortex_start=$(date +%s.%N)
}

__cortex_precmd() {
    local exit_code=$?
    [[ -n "$__cortex_cmd" ]] || return

    local now duration
    now=$(date +%s.%N)
    duration=$(echo "$now - $__cortex_start" | bc 2>/dev/null || echo "0")

    local branch=""
    branch=$(git branch --show-current 2>/dev/null)

    local pipe="${XDG_RUNTIME_DIR:-/tmp}/cortex/terminal.pipe"
    if [[ -p "$pipe" ]]; then
        printf '{"cmd":"%s","exit":%d,"dur":%s,"cwd":"%s","branch":"%s"}\n' \
            "${__cortex_cmd//\"/\\\"}" "$exit_code" "$duration" "$PWD" "$branch" \
            > "$pipe" 2>/dev/null
    fi

    unset __cortex_cmd __cortex_start
}

# bash-preexec compatible hook
if [[ -n "$BASH_VERSION" ]]; then
    trap '__cortex_preexec "$BASH_COMMAND"' DEBUG
    PROMPT_COMMAND="__cortex_precmd;${PROMPT_COMMAND}"
fi
