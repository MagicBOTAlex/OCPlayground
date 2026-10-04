#!/usr/bin/env bash
# Vendors the OpenComputers system Lua (machine.lua, bios.lua, OpenOS) into
# assets/system. Re-run this to refresh from a newer OpenComputers checkout.
#
# Usage:
#   scripts/fetch-system.sh [path-to-OpenComputers]
#
# If no path is given, the sibling ../OpenComputers checkout is used if it
# exists, otherwise the files are fetched from GitHub.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/assets/system"
OC_REPO="${1:-$ROOT/../OpenComputers}"
REMOTE="https://github.com/MightyPirates/OpenComputers.git"

if [ -d "$OC_REPO/src/main/resources/assets/opencomputers" ]; then
  ASSETS="$OC_REPO/src/main/resources/assets/opencomputers"
  echo "Vendoring from $ASSETS"
else
  ASSETS="$(mktemp -d)/opencomputers"
  echo "Fetching system files from $REMOTE"
  git clone --depth=1 --filter=blob:none --sparse "$REMOTE" "$ASSETS/repo" >/dev/null
  git -C "$ASSETS/repo" sparse-checkout set src/main/resources/assets/opencomputers >/dev/null
  ASSETS="$ASSETS/repo/src/main/resources/assets/opencomputers"
fi

rm -rf "$DEST"
mkdir -p "$DEST/loot"
cp "$ASSETS/lua/machine.lua" "$DEST/machine.lua"
cp "$ASSETS/lua/bios.lua" "$DEST/bios.lua"
cp -R "$ASSETS/loot/openos" "$DEST/loot/openos"

echo "Done: $DEST"
