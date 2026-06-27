#!/usr/bin/env bash
set -euo pipefail

REPO="ShiinaSaku/Hayate"
BINARY_NAME="hayate"

# High-fidelity modern ANSI terminal colors
CYAN='\033[38;5;45m'
PURPLE='\033[38;5;141m'
GREEN='\033[38;5;84m'
RED='\033[38;5;203m'
GRAY='\033[38;5;244m'
NC='\033[0m'

# Visual Header
echo -e "${CYAN}"
cat << "EOF"
    __  _______  _____  ____________
   / / / /   \ \/ /   |/_  __/ ____/
  / /_/ / /| |\  / /| | / / / __/   
 / __  / ___ |/ / ___ |/ / / /___   
/_/ /_/_/  |_/_/_/  |_/_/ /_____/   
                                    
EOF
echo -e "${PURPLE}  Swift, Secure, Encrypted & Compressed Local File Transfers${NC}\n"

log_info() { echo -e "${CYAN}[*]${NC} $1"; }
log_success() { echo -e "${GREEN}[+]${NC} $1"; }
log_error() { echo -e "${RED}[-]${NC} $1" >&2; exit 1; }

# OS and Arch detection
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

# Check for required commands
if ! command -v curl >/dev/null 2>&1 && ! command -v wget >/dev/null 2>&1; then
    log_error "This script requires 'curl' or 'wget'. Please install one to proceed."
fi

log_info "Detected target: ${OS_TARGET}-${ARCH_TARGET}"

# Determine installation directory and privileges
if [ -n "${PREFIX:-}" ] && [[ "$PREFIX" == *com.termux* ]]; then
    log_info "Termux environment detected."
    INSTALL_DIR="$PREFIX/bin"
    USE_SUDO=""
else
    INSTALL_DIR="/usr/local/bin"
    if [ -w "$INSTALL_DIR" ]; then
        USE_SUDO=""
    else
        log_info "Elevated privileges required to write to ${INSTALL_DIR}"
        USE_SUDO="sudo"
    fi
fi

# Resolve latest release version tag
log_info "Resolving latest version from GitHub..."
if command -v curl >/dev/null 2>&1; then
    LATEST_TAG=$(curl -sLI -o /dev/null -w "%{url_effective}" "https://github.com/${REPO}/releases/latest" | sed 's|.*/||')
else
    LATEST_TAG=$(wget --max-redirect=0 "https://github.com/${REPO}/releases/latest" 2>&1 | grep "Location:" | sed 's|.*/||' || true)
fi

if [ -z "$LATEST_TAG" ] || [ "$LATEST_TAG" = "latest" ]; then
    log_error "Failed to fetch latest release tag. Check your internet connection."
fi

# Short-circuit if already up-to-date
if command -v "$BINARY_NAME" >/dev/null 2>&1; then
    CURRENT_VERSION=$("$BINARY_NAME" --version 2>/dev/null | cut -d' ' -f1 || true)
    if [ "$CURRENT_VERSION" = "$LATEST_TAG" ]; then
        log_success "Hayate is already up-to-date (${CURRENT_VERSION})!"
        exit 0
    fi
fi

# Setup asset download url
if [ -n "${PREFIX:-}" ] && [[ "$PREFIX" == *com.termux* ]] && [ "$ARCH_TARGET" = "arm64" ]; then
    ASSET_NAME="${BINARY_NAME}-termux-arm64"
else
    ASSET_NAME="${BINARY_NAME}-${OS_TARGET}-${ARCH_TARGET}"
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${ASSET_NAME}"
TMP_DIR=$(mktemp -d)
TMP_BIN="${TMP_DIR}/${BINARY_NAME}"

log_info "Downloading ${ASSET_NAME} (${LATEST_TAG})..."
if command -v curl >/dev/null 2>&1; then
    curl -# -sLf -o "$TMP_BIN" "$DOWNLOAD_URL"
else
    wget -q --show-progress -O "$TMP_BIN" "$DOWNLOAD_URL"
fi

log_info "Installing binary to ${INSTALL_DIR}..."
chmod +x "$TMP_BIN"

if ! $USE_SUDO mv "$TMP_BIN" "${INSTALL_DIR}/${BINARY_NAME}"; then
    log_error "Failed to move binary to ${INSTALL_DIR}. Rerun with sudo or check directory permissions."
fi

rm -rf "$TMP_DIR"

log_success "Hayate ${LATEST_TAG} installed successfully!"
echo -e "${GRAY}Get started by running 'hayate help' or 'hayate send <file>'${NC}"
