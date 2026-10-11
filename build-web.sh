#!/usr/bin/env bash

ROBSIM_PROJECT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)" || return 1 2>/dev/null || exit 1

robsim_build_web() (
    cd -- "$ROBSIM_PROJECT_DIR" || return 1

    if ! command -v trunk >/dev/null 2>&1; then
        printf '%s\n' 'Trunk is required. Install it with: cargo install trunk --locked' >&2
        return 1
    fi

    NO_COLOR=true trunk build --release --locked --public-url ./ --dist "$ROBSIM_PROJECT_DIR/dist" || return "$?"

    if [[ ! -f dist/index.html ]]; then
        printf '%s\n' 'Build did not produce dist/index.html.' >&2
        return 1
    fi

    printf '\nWeb build ready: %s/dist\nUpload the entire directory, including assets.\n' "$ROBSIM_PROJECT_DIR"
)

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    robsim_build_web "$@"
fi
