#!/usr/bin/env zsh
# Ambient Cortex — zsh shell integration
# Source this file in your .zshrc:
#   source ~/.config/cortex/cortex-preexec.zsh

__cortex_preexec() {
    __cortex_cmd="$1"
    __cortex_start=$EPOCHREALTIME
}

__cortex_precmd() {
    local exit_code=$?
    [[ -n "$__cortex_cmd" ]] || return

    local duration=0
    if [[ -n "$__cortex_start" ]]; then
        duration=$(( EPOCHREALTIME - __cortex_start ))
    fi

    local branch=""
    branch=$(git branch --show-current 2>/dev/null)

    local pipe="${XDG_RUNTIME_DIR:-/tmp}/cortex/terminal.pipe"
    if [[ -p "$pipe" ]]; then
        printf '{"cmd":"%s","exit":%d,"dur":%.3f,"cwd":"%s","branch":"%s"}\n' \
            "${__cortex_cmd//\"/\\\"}" "$exit_code" "$duration" "$PWD" "$branch" \
            > "$pipe" 2>/dev/null
    fi

    unset __cortex_cmd __cortex_start
}

# Display cortex insight in prompt if available
__cortex_prompt() {
    local insight_file="${HOME}/.local/share/cortex/current_insight.json"
    if [[ -f "$insight_file" ]]; then
        local title
        title=$(command cat "$insight_file" 2>/dev/null | command grep -o '"title":"[^"]*"' | head -1 | cut -d'"' -f4)
        if [[ -n "$title" ]]; then
            echo "cortex> $title"
        fi
    fi
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec __cortex_preexec
add-zsh-hook precmd __cortex_precmd
add-zsh-hook precmd __cortex_prompt
