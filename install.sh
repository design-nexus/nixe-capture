#!/usr/bin/env bash
set -euo pipefail

# ANSI color codes
BOLD="\033[1m"
GREEN="\033[0;32m"
BLUE="\033[0;34m"
YELLOW="\033[0;33m"
RED="\033[0;31m"
RESET="\033[0m"

echo -e "${BOLD}${BLUE}==> Nexus Photos Installer${RESET}"

# Check required dependencies
check_dep() {
    local cmd="$1"
    local pkg_hint="$2"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${RED}Error: Required command '$cmd' is not installed.${RESET}"
        echo -e "${YELLOW}Please install it first (e.g. 'sudo pacman -S $pkg_hint').${RESET}"
        exit 1
    fi
}

check_dep "cargo" "rust"
check_dep "git" "git"
check_dep "quickshell" "quickshell"

# Determine source directory (local repo or clone via curl pipe)
CLEANUP_TEMP=false
SRC_DIR=""

if [ -f "Cargo.toml" ] && [ -d "ui" ] && [ -f "ui/shell.qml" ]; then
    SRC_DIR="$(pwd)"
elif [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "$(dirname "${BASH_SOURCE[0]}")/Cargo.toml" ]; then
    SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    echo -e "${BLUE}==> Downloading Nexus Photos from GitHub...${RESET}"
    TEMP_DIR="$(mktemp -d /tmp/nexus-photos-install.XXXXXX)"
    CLEANUP_TEMP=true
    trap 'if [ "$CLEANUP_TEMP" = true ]; then rm -rf "$TEMP_DIR"; fi' EXIT
    git clone --depth 1 https://github.com/design-nexus/nexus-gallery.git "$TEMP_DIR"
    SRC_DIR="$TEMP_DIR"
fi

cd "$SRC_DIR"

# Build Rust backend
echo -e "${BLUE}==> Compiling backend binary (release mode)...${RESET}"
cargo build --release

# Install binary
echo -e "${BLUE}==> Installing binary to ~/.local/bin/nexus-photos...${RESET}"
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/nexus-photos "$HOME/.local/bin/nexus-photos"

# Install UI assets
echo -e "${BLUE}==> Installing UI components to ~/.local/share/nexus-photos/ui...${RESET}"
mkdir -p "$HOME/.local/share/nexus-photos/ui"
cp -r ui/* "$HOME/.local/share/nexus-photos/ui/"

# Install Application Icon
echo -e "${BLUE}==> Installing application icons...${RESET}"
mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps"
cp assets/nexus-photos.svg "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.nexus.photos.svg"
cp assets/nexus-photos.svg "$HOME/.local/share/icons/hicolor/scalable/apps/nexus-photos.svg"

# Install Desktop Entry
echo -e "${BLUE}==> Installing desktop entry...${RESET}"
mkdir -p "$HOME/.local/share/applications"
cp nexus-photos.desktop "$HOME/.local/share/applications/nexus-photos.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true
fi

# PATH check
echo ""
echo -e "${GREEN}${BOLD}✔ Successfully installed Nexus Photos!${RESET}"
echo ""

if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    echo -e "${YELLOW}Notice: '$HOME/.local/bin' is not in your current PATH.${RESET}"
    echo -e "${YELLOW}Add it to your shell config (~/.bashrc or ~/.zshrc):${RESET}"
    echo -e "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
fi

echo -e "You can run Nexus Photos by:"
echo -e "  • Typing ${BOLD}nexus-photos${RESET} in your terminal"
echo -e "  • Launching ${BOLD}Nexus Photos${RESET} from your application launcher (Rofi, Walker, Fuzzel, etc.)"
