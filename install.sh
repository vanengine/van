#!/bin/sh
set -e

REPO="vanengine/van"
INSTALL_DIR="/usr/local/bin"
BINARY_NAME="van"

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux*)  OS_TAG="linux" ;;
    Darwin*) OS_TAG="darwin" ;;
    *)
        echo "Error: Unsupported OS: $OS"
        echo "Windows users: download manually from https://github.com/$REPO/releases"
        exit 1
        ;;
esac

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)  ARCH_TAG="x64" ;;
    aarch64|arm64) ARCH_TAG="arm64" ;;
    *)
        echo "Error: Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

ARTIFACT="van-cli-${OS_TAG}-${ARCH_TAG}"
echo "Detected platform: ${OS_TAG}-${ARCH_TAG}"

# Get latest release download URL from GitHub API
DOWNLOAD_URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep "browser_download_url.*$ARTIFACT\"" \
    | head -1 \
    | cut -d '"' -f 4)

if [ -z "$DOWNLOAD_URL" ]; then
    echo "Error: Could not find release artifact: $ARTIFACT"
    echo "Check available releases at https://github.com/$REPO/releases"
    exit 1
fi

echo "Downloading $ARTIFACT ..."
curl -fSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo "Installed to $INSTALL_DIR/$BINARY_NAME"
van --version
