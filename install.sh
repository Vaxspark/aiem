#!/bin/sh
# aiem one-line installer for Linux / macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/Vaxspark/aiem/main/install.sh | sh
#
# Default behavior uses the native installer when available:
# - macOS: .pkg
# - Debian/Ubuntu: .deb
# Set AIEM_INSTALL_MODE=portable to install the tarball into ~/.local/bin.

set -eu

REPO="Vaxspark/aiem"
INSTALL_DIR="${AIEM_INSTALL_DIR:-$HOME/.local/bin}"
INSTALL_MODE="${AIEM_INSTALL_MODE:-native}"

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required" >&2
    exit 1
  fi
}

as_root() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    echo "This step requires root privileges. Install sudo or run as root." >&2
    exit 1
  fi
}

fetch_url() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- "$1"
  else
    echo "curl or wget is required" >&2
    exit 1
  fi
}

download_file() {
  if command -v curl >/dev/null 2>&1; then
    curl -fL "$1" -o "$2"
  else
    wget -O "$2" "$1"
  fi
}

asset_url() {
  pattern="$1"
  printf '%s\n' "$RELEASE_JSON" |
    sed -n 's/.*"browser_download_url": "\(.*\)".*/\1/p' |
    grep "${pattern}$" |
    head -n 1
}

install_tarball() {
  pattern="$1"
  url="$(asset_url "$pattern")"
  if [ -z "$url" ]; then
    echo "Could not find release asset matching: $pattern" >&2
    exit 1
  fi

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  echo "Downloading $(basename "$url") ..."
  download_file "$url" "$tmp/aiem.tar.gz"
  tar -xzf "$tmp/aiem.tar.gz" -C "$tmp"

  aiem_bin="$(find "$tmp" -type f -name aiem | head -n 1)"
  gui_bin="$(find "$tmp" -type f -name aiem-gui | head -n 1 || true)"

  if [ -z "$aiem_bin" ]; then
    echo "aiem binary was not found in the archive" >&2
    exit 1
  fi

  mkdir -p "$INSTALL_DIR"
  cp "$aiem_bin" "$INSTALL_DIR/aiem"
  chmod +x "$INSTALL_DIR/aiem"

  if [ -n "$gui_bin" ]; then
    cp "$gui_bin" "$INSTALL_DIR/aiem-gui"
    chmod +x "$INSTALL_DIR/aiem-gui"
  fi

  echo "Installed aiem to $INSTALL_DIR"
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      echo ""
      echo "NOTE: $INSTALL_DIR is not in your PATH."
      echo "Add this line to your shell profile:"
      echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
      ;;
  esac
}

API_URL="https://api.github.com/repos/${REPO}/releases/latest"
echo "Fetching latest aiem release..."
RELEASE_JSON="$(fetch_url "$API_URL")"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin*)
    case "$ARCH" in
      arm64|aarch64) NATIVE_PATTERN="macos-arm64.pkg"; TARBALL_PATTERN="macos-arm64.tar.gz" ;;
      x86_64|amd64) NATIVE_PATTERN="macos-x86_64.pkg"; TARBALL_PATTERN="macos-x86_64.tar.gz" ;;
      *) echo "Unsupported macOS architecture: $ARCH" >&2; exit 1 ;;
    esac

    if [ "$INSTALL_MODE" != "portable" ]; then
      url="$(asset_url "$NATIVE_PATTERN")"
      if [ -n "$url" ]; then
        tmp="$(mktemp -d)"
        trap 'rm -rf "$tmp"' EXIT
        pkg="$tmp/aiem.pkg"
        echo "Downloading $(basename "$url") ..."
        download_file "$url" "$pkg"
        echo "Installing aiem pkg..."
        as_root installer -pkg "$pkg" -target /
        echo "Installed aiem. Open aiem from Applications or run aiem in Terminal."
        exit 0
      fi
      echo "macOS pkg was not found; falling back to portable tarball install."
    fi

    install_tarball "$TARBALL_PATTERN"
    ;;

  Linux*)
    case "$ARCH" in
      x86_64|amd64) DEB_PATTERN="linux-x86_64-gnu.deb"; TARBALL_PATTERN="linux-x86_64-musl.tar.gz" ;;
      *) echo "Unsupported Linux architecture: $ARCH" >&2; exit 1 ;;
    esac

    if [ "$INSTALL_MODE" != "portable" ] && command -v dpkg >/dev/null 2>&1; then
      url="$(asset_url "$DEB_PATTERN")"
      if [ -n "$url" ]; then
        tmp="$(mktemp -d)"
        trap 'rm -rf "$tmp"' EXIT
        deb="$tmp/aiem.deb"
        echo "Downloading $(basename "$url") ..."
        download_file "$url" "$deb"
        echo "Installing aiem deb..."
        if ! as_root dpkg -i "$deb"; then
          if command -v apt-get >/dev/null 2>&1; then
            as_root apt-get install -f -y
          else
            exit 1
          fi
        fi
        echo "Installed aiem. Launch aiem from your app menu or run aiem in Terminal."
        exit 0
      fi
      echo "Debian package was not found; falling back to portable tarball install."
    fi

    install_tarball "$TARBALL_PATTERN"
    ;;

  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

echo "Run 'aiem init' to get started."
