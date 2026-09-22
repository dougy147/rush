#!/usr/bin/env bash

_rush() { 
    local tmp=$(mktemp)
    if command -v rush >/dev/null; then 
        rush | tee -pi $tmp
    else
        ./rush | tee -pi $tmp # grab echoed last line from ./rush
    fi
    echo -e "\r\e[1A\r"   # erase ./rush last line from terminal
    output="$(tail -n1 $tmp | sed 's/^ *$//')"
    if [[ -n output ]]; then
        READLINE_LINE="$output"
        READLINE_POINT=0x7fffffff
    fi
    rm $tmp
}
bind -x '"\C-r": _rush'
