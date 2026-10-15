#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

run_ci_gates() {
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
}

install_release_binary() {
    cargo install --path . --locked
}

report_installed_version() {
    local installed
    installed="$(command -v rx || true)"
    if [[ "$installed" != "$HOME/.cargo/bin/rx" ]]; then
        echo "warning: rx resolves to '${installed:-nothing}', not $HOME/.cargo/bin/rx" >&2
    fi
    echo "rx installed from $(git rev-parse --short HEAD)"
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "warning: the working tree has uncommitted changes, they are included in this build" >&2
    fi
}

case "${1:-}" in
    --skip-checks) ;;
    "") run_ci_gates ;;
    *) echo "usage: scripts/install.sh [--skip-checks]" >&2; exit 2 ;;
esac

install_release_binary
report_installed_version
