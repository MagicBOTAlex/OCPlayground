#!/usr/bin/env bash
# Install ocplay with Nix (flakes).
#
#   scripts/install.sh              install / upgrade ocplay
#   scripts/install.sh --uninstall  remove it again
set -euo pipefail

cd "$(dirname "$0")/.."

nix_flags=(--extra-experimental-features "nix-command flakes")

if [ "${1:-}" = "--uninstall" ]; then
  exec nix "${nix_flags[@]}" profile remove ocplay
fi

exec nix "${nix_flags[@]}" profile install "$(pwd)#ocplay" "$@"
