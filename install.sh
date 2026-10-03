#!/bin/bash
# Install Nexus Photos, a photo gallery for Omarchy.
#
#   curl -fsSL https://raw.githubusercontent.com/design-nexus/nexus-gallery/main/install.sh | bash
#
# From a clone, ./install.sh builds from source.
set -euo pipefail

REPO="design-nexus/nexus-gallery"

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m::\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m::\033[0m %s\n' "$*" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

script_dir=""
if [[ -n ${BASH_SOURCE[0]:-} && -f ${BASH_SOURCE[0]} ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi

if [[ -n $script_dir && -f $script_dir/Cargo.toml ]]; then
  src="$script_dir"
else
  command -v git >/dev/null || die "git is needed to fetch the source."
  say "Downloading Nexus Photos"
  git clone --depth 1 "https://github.com/$REPO.git" "$work/src"
  src="$work/src"
fi

if ! command -v cargo >/dev/null; then
  if command -v pacman >/dev/null; then
    say "Installing Rust and GTK 4 build dependencies"
    sudo pacman -S --needed --noconfirm rust gtk4 pkgconf gcc
  else
    die "Rust (cargo) is needed to build Nexus Photos. Install it and run this again."
  fi
fi

say "Building Nexus Photos (a minute or two the first time)"
(cd "$src" && cargo build --release --locked)

bin="$HOME/.local/bin"
apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/scalable/apps"
mkdir -p "$bin" "$apps" "$icons"

say "Installing to ~/.local"
install -m 755 "$src/target/release/nexus-photos" "$bin/nexus-photos"
install -m 755 "$src/target/release/nexus-photos-viewer" "$bin/nexus-photos-viewer"
install -m 644 "$src/nexus-photos.desktop" "$apps/nexus-photos.desktop"
install -m 644 "$src/nexus-photos-viewer.desktop" "$apps/nexus-photos-viewer.desktop"
install -m 644 "$src/assets/nexus-photos.svg" "$icons/io.github.nexus.photos.svg"
update-desktop-database "$apps" 2>/dev/null || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

command -v ffmpegthumbnailer >/dev/null || command -v ffmpeg >/dev/null ||
  warn "Optional: 'sudo pacman -S ffmpegthumbnailer' adds thumbnails for videos."

case ":$PATH:" in
  *":$bin:"*) ;;
  *) warn "$bin isn't on your PATH; launch Nexus Photos from the app launcher, or add it to PATH." ;;
esac

say "Done. Open Nexus Photos from the app launcher, or run: nexus-photos"
