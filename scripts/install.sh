#!/usr/bin/env bash
set -e

REPO="ShiinaSaku/Hayate"
BINARY_NAME="hayate"

# High-fidelity terminal colors
CYAN='\033[0;36m'
INDIGO='\033[0;35m'
GREEN='\033[0;32m'
RED='\033[0;31m'
GRAY='\033[1;30m'
NC='\033[0m'

echo -e "${CYAN}"
cat << "EOF"
  __   __     _____    __  __    _____    _______     _____  
 /\_\ /_/\   /\___/\ /\  /\  /\ /\___/\ /\_______)\ /\_____\ 
( ( (_) ) ) / / _ \ \\ \ \/ / // / _ \ \\(___  __\/( (_____/ 
 \ \___/ /  \ \(_)/ / \ \__/ / \ \(_)/ /  / / /     \ \__\   
 / / _ \ \  / / _ \ \  \__/ /  / / _ \ \ ( ( (      / /__/_  
( (_( )_) )( (_( )_) ) / / /  ( (_( )_) ) \ \ \    ( (_____\ 
 \/_/ \_\/  \/_/ \_\/  \/_/    \/_/ \_\/  /_/_/     \/_____/ 
EOF
echo -e "  Swift File Transfer | Secure, Encrypted, & Compressed${NC}\n"

log_info() { echo -e "${CYAN}[*]${NC} $1"; }
log_success() { echo -e "${GREEN}[+]${NC} $1"; }
log_error() { echo -e "${RED}[-]${NC} $1"; exit 1; }

# OS detection
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$OS" in
    linux) OS_TARGET="linux" ;;
    darwin) OS_TARGET="darwin" ;;
    *) log_error "Unsupported operating system: $OS" ;;
esac

# Architecture detection
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64) ARCH_TARGET="amd64" ;;
    aarch64|arm64) ARCH_TARGET="arm64" ;;
    armv7l|armv8l|arm) ARCH_TARGET="arm" ;;
    *) log_error "Unsupported architecture: $ARCH" ;;
esac

log_info "Detected target environment: ${OS_TARGET}-${ARCH_TARGET}"

# Determine installation directory and if sudo is required
if [ -n "$PREFIX" ] && [[ "$PREFIX" == *com.termux* ]]; then
    log_info "Termux environment detected."
    INSTALL_DIR="$PREFIX/bin"
    USE_SUDO=""
else
    INSTALL_DIR="/usr/local/bin"
    if [ -w "$INSTALL_DIR" ]; then
        USE_SUDO=""
    else
        log_info "Elevated privileges required to install to ${INSTALL_DIR}"
        USE_SUDO="sudo"
    fi
fi

log_info "Resolving latest release tag from GitHub..."
# Rate-limit proof redirection resolver
LATEST_TAG=$(curl -sLI -o /dev/null -w "%{url_effective}" "https://github.com/${REPO}/releases/latest" | sed 's|.*/||')

if [ -z "$LATEST_TAG" ] || [ "$LATEST_TAG" = "latest" ]; then
    log_error "Failed to fetch latest release tag. Please verify network or repository settings."
fi

# Select target build asset
if [ -n "$PREFIX" ] && [[ "$PREFIX" == *com.termux* ]] && [ "$ARCH_TARGET" = "arm64" ]; then
    ASSET_NAME="${BINARY_NAME}-termux-arm64"
else
    ASSET_NAME="${BINARY_NAME}-${OS_TARGET}-${ARCH_TARGET}"
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${ASSET_NAME}"
TMP_DIR=$(mktemp -d)
TMP_BIN="${TMP_DIR}/${BINARY_NAME}"

log_info "Downloading ${ASSET_NAME} (${LATEST_TAG})..."
if ! curl -# -sLf -o "$TMP_BIN" "$DOWNLOAD_URL"; then
    log_error "Download failed. Please check connection and verify if binary exists for this platform."
fi

log_info "Configuring permissions and installing to ${INSTALL_DIR}..."
chmod +x "$TMP_BIN"

if ! $USE_SUDO mv "$TMP_BIN" "${INSTALL_DIR}/${BINARY_NAME}"; then
    log_error "Failed to move binary to ${INSTALL_DIR}. Check permissions or rerun with sudo."
fi

rm -rf "$TMP_DIR"

log_success "Hayate ${LATEST_TAG} installed successfully!"
echo -e "${GRAY}Get started by running 'hayate help' or 'hayate send <file>'${NC}"
