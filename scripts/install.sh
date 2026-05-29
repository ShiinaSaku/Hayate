#!/usr/bin/env bash
set -e

REPO="ShiinaSaku/Hayate"
BINARY_NAME="hayate"

CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
GRAY='\033[1;30m'
NC='\033[0m'

echo -e "${CYAN}"
cat << "EOF"
  _   _    _ __   __  _  _____  ___ 
 | | | |  / \ \ / / / \|_   _|/ _ \
 | |_| | / _ \ \ V / / _ \ | | |  _/
 |  _  |/ ___ \ | | / ___ \| | | |  
 |_| |_/_/   \_\_|/_/   \_\_| \___|
EOF
echo -e " Swift Cross-Device File Transfer${NC}\n"

log_info() { echo -e "${GRAY}[*]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_error() { echo -e "${RED}[ERR]${NC} $1"; exit 1; }

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$OS" in
    linux) OS_TARGET="linux" ;;
    darwin) OS_TARGET="darwin" ;;
    *) log_error "Unsupported operating system: $OS" ;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64) ARCH_TARGET="amd64" ;;
    aarch64|arm64) ARCH_TARGET="arm64" ;;
    armv7l|armv8l|arm) ARCH_TARGET="arm" ;;
    *) log_error "Unsupported architecture: $ARCH" ;;
esac

log_info "Detected environment: $OS_TARGET-$ARCH_TARGET"

if [ -n "$PREFIX" ] && [[ "$PREFIX" == *com.termux* ]]; then
    log_info "Termux environment detected."
    INSTALL_DIR="$PREFIX/bin"
    USE_SUDO=""
else
    INSTALL_DIR="/usr/local/bin"
    if [ -w "$INSTALL_DIR" ]; then
        USE_SUDO=""
    else
        log_info "Elevated privileges required to install to $INSTALL_DIR"
        USE_SUDO="sudo"
    fi
fi

log_info "Fetching latest release metadata..."
# Rate-limit proof: Bypasses API entirely and extracts tag from redirect URL
LATEST_TAG=$(curl -sLI -o /dev/null -w "%{url_effective}" "https://github.com/${REPO}/releases/latest" | sed 's|.*/||')

if [ -z "$LATEST_TAG" ] || [ "$LATEST_TAG" = "latest" ]; then
    log_error "Failed to fetch the latest release tag. Check repository visibility."
fi

ASSET_NAME="${BINARY_NAME}-${OS_TARGET}-${ARCH_TARGET}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${ASSET_NAME}"
TMP_DIR=$(mktemp -d)
TMP_BIN="${TMP_DIR}/${BINARY_NAME}"

log_info "Downloading ${ASSET_NAME} (${LATEST_TAG})..."
if ! curl -# -sLf -o "$TMP_BIN" "$DOWNLOAD_URL"; then
    log_error "Download failed, Please check your network connection."
fi

log_info "Installing to ${INSTALL_DIR}..."
chmod +x "$TMP_BIN"

if ! $USE_SUDO mv "$TMP_BIN" "${INSTALL_DIR}/${BINARY_NAME}"; then
    log_error "Failed to move binary to $INSTALL_DIR. Do you have the right permissions?"
fi

rm -rf "$TMP_DIR"

log_success "Hayate ${LATEST_TAG} installed successfully."
echo -e "${GRAY}Run 'hayate --help' to get started.${NC}"
