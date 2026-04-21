#!/bin/sh
# aiem one-line installer for Linux / macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh

set -e

REPO="Vaxspark/aiem"
BIN_NAME="aiem"
INSTALL_DIR="${AIEM_INSTALL_DIR:-$HOME/.local/bin}"

# ── detect platform ────────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux*)
    ASSET="aiem-linux-x86_64-musl.tar.gz"
    ;;
  Darwin*)
    # macOS — fall back to the Linux musl binary via Rosetta / native x86_64
    ASSET="aiem-linux-x86_64-musl.tar.gz"
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

# ── fetch latest release URL ───────────────────────────────────────────────────
API_URL="https://api.github.com/repos/${REPO}/releases/latest"

if command -v curl >/dev/null 2>&1; then
  DOWNLOAD_URL="$(curl -fsSL "$API_URL" | grep "browser_download_url" | grep "$ASSET" | sed 's/.*"browser_download_url": "\(.*\)".*/\1/')"
elif command -v wget >/dev/null 2>&1; then
  DOWNLOAD_URL="$(wget -qO- "$API_URL" | grep "browser_download_url" | grep "$ASSET" | sed 's/.*"browser_download_url": "\(.*\)".*/\1/')"
else
  echo "curl or wget is required" >&2
  exit 1
fi

if [ -z "$DOWNLOAD_URL" ]; then
  echo "Failed to resolve download URL" >&2
  exit 1
fi

# ── download & install ─────────────────────────────────────────────────────────
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading $ASSET ..."
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$DOWNLOAD_URL" -o "$TMP/aiem.tar.gz"
else
  wget -qO "$TMP/aiem.tar.gz" "$DOWNLOAD_URL"
fi

tar -xzf "$TMP/aiem.tar.gz" -C "$TMP"
mkdir -p "$INSTALL_DIR"
cp "$TMP/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"

echo "Installed to $INSTALL_DIR/$BIN_NAME"

# ── PATH hint ─────────────────────────────────────────────────────────────────
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "  NOTE: $INSTALL_DIR is not in your PATH."
    echo "  Add the following line to your shell profile (~/.bashrc / ~/.zshrc):"
    echo ""
    echo '    export PATH="$HOME/.local/bin:$PATH"'
    echo ""
    ;;
esac

echo "Run 'aiem init' to get started."
