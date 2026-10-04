#!/usr/bin/env bash
#
# Easy installer for ocplay on Ubuntu / generic x86_64 Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/MagicBOTAlex/OCPlayground/master/install.sh | bash
#
# or, from a checkout:
#
#   ./install.sh [--version v0.1.0] [--prefix ~/.local]
#
set -euo pipefail

REPO="${OCPLAY_REPO:-MagicBOTAlex/OCPlayground}"
VERSION="${OCPLAY_VERSION:-latest}"
PREFIX="${OCPLAY_PREFIX:-$HOME/.local}"

usage() {
  cat <<EOF
Install ocplay (latest GitHub build) for x86_64 Linux/Ubuntu.

Usage: install.sh [options]

Options:
  --version <tag>   Release tag to install (default: latest)
  --prefix <dir>    Install prefix (default: \$HOME/.local)
  -h, --help        Show this help

Environment:
  OCPLAY_REPO       GitHub repository (default: $REPO)
  OCPLAY_VERSION    Same as --version
  OCPLAY_PREFIX     Same as --prefix
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:?error: --version needs a value}"; shift 2 ;;
    --prefix)  PREFIX="${2:?error: --prefix needs a value}";  shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown option '$1'" >&2; usage >&2; exit 1 ;;
  esac
done

case "$(uname -s)" in
  Linux) ;;
  *) echo "error: this installer only supports Linux" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64) target=x86_64-unknown-linux-gnu ;;
  *) echo "error: unsupported architecture '$(uname -m)'; only x86_64 builds are published" >&2; exit 1 ;;
esac

asset="ocplay-$target.tar.gz"
if [ "$VERSION" = "latest" ]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi
url="$base/$asset"

share="$PREFIX/share/ocplay"
bindir="$PREFIX/bin"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

download() {
  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    echo "error: need either curl or wget" >&2
    exit 1
  fi
}

echo "Downloading $url"
download "$url" "$tmp/$asset"

if download "$url.sha256" "$tmp/$asset.sha256" 2>/dev/null; then
  echo "Verifying checksum..."
  ( cd "$tmp" && sha256sum -c "$asset.sha256" )
else
  echo "note: no checksum published, skipping verification"
fi

rm -rf "$share"
mkdir -p "$share" "$bindir"
tar -xzf "$tmp/$asset" -C "$share" --strip-components=1

ln -sf "$share/ocplay" "$bindir/ocplay"

echo "Installed ocplay -> $bindir/ocplay"

case ":$PATH:" in
  *":$bindir:"*) ;;
  *)
    echo
    echo "Add it to your PATH with:"
    echo "  echo 'export PATH=\"$bindir:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
    ;;
esac

if "$bindir/ocplay" --help >/dev/null 2>&1; then
  echo "ocplay is ready."
else
  echo "warning: ocplay did not run; check your platform (glibc version, architecture)" >&2
fi
