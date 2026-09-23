#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "==> Building Nexus Photos (release)..."
cargo build --release

echo "==> Installing binary to ~/.local/bin/nexus-photos..."
mkdir -p ~/.local/bin
cp target/release/nexus-photos ~/.local/bin/nexus-photos
chmod +x ~/.local/bin/nexus-photos

echo "==> Installing UI components to ~/.local/share/nexus-photos/ui..."
mkdir -p ~/.local/share/nexus-photos/ui
cp -r ui/* ~/.local/share/nexus-photos/ui/

echo "==> Installing icons to ~/.local/share/icons/hicolor/scalable/apps..."
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
cp assets/nexus-photos.svg ~/.local/share/icons/hicolor/scalable/apps/io.github.nexus.photos.svg
cp assets/nexus-photos.svg ~/.local/share/icons/hicolor/scalable/apps/nexus-photos.svg

echo "==> Installing desktop entry to ~/.local/share/applications/nexus-photos.desktop..."
mkdir -p ~/.local/share/applications
cp nexus-photos.desktop ~/.local/share/applications/nexus-photos.desktop

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database ~/.local/share/applications
fi

echo "==> Successfully installed Nexus Photos!"
echo "    Run 'nexus-photos' from your terminal or launch it from your Omarchy app menu."
