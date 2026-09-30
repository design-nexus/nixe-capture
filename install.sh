#!/bin/bash
# Install Torrents, a BitTorrent client for Omarchy on qBittorrent's engine.
#
# From a clone, ./install.sh builds from source.
#
# Options:
#   --default   also make Torrents the app for magnet links and .torrent files
set -euo pipefail

REPO="design-nexus/nexus-torrents"

make_default=false
for arg in "$@"; do
  case "$arg" in
    --default) make_default=true ;;
    -h | --help)
      sed -n '2,8p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m::\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m::\033[0m %s\n' "$*" >&2; exit 1; }

command -v hyprctl >/dev/null || warn "Hyprland wasn't found. Torrents is made for Omarchy (Hyprland)."

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

script_dir=""
if [[ -n ${BASH_SOURCE[0]:-} && -f ${BASH_SOURCE[0]} ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi

src="$script_dir"
if [[ -z $src || ! -f $src/Cargo.toml ]]; then
  command -v git >/dev/null || die "git is needed to fetch the source."
  git clone --depth 1 "https://github.com/$REPO.git" "$work/src"
  src="$work/src"
fi

# libtorrent-rasterbar is the engine (the same one qBittorrent uses); its headers need Boost.
if command -v pacman >/dev/null; then
  missing=()
  for pkg in rust gtk4 pkgconf gcc libtorrent-rasterbar boost; do
    pacman -Q "$pkg" >/dev/null 2>&1 || missing+=("$pkg")
  done
  if ((${#missing[@]})); then
    say "Installing build dependencies: ${missing[*]}"
    sudo pacman -S --needed --noconfirm "${missing[@]}"
  fi
else
  command -v cargo >/dev/null || die "Rust (cargo) is needed to build Torrents."
  pkg-config --exists libtorrent-rasterbar || die "libtorrent-rasterbar 2.x (and Boost headers) are needed to build Torrents."
fi

say "Building Torrents (a few minutes the first time)"
(cd "$src" && cargo build --release --locked)

bin="$HOME/.local/bin"
apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/scalable/apps"
mkdir -p "$bin" "$apps" "$icons"

say "Installing to ~/.local"
install -m 755 "$src/target/release/torrents" "$bin/torrents"
install -m 644 "$src/data/io.github.design_nexus.Torrents.desktop" "$apps/"
install -m 644 "$src/data/io.github.design_nexus.Torrents.svg" "$icons/"
update-desktop-database "$apps" 2>/dev/null || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

command -v python3 >/dev/null || warn "Optional: python3 isn't installed; search plugins need it."
command -v curl >/dev/null || warn "Optional: curl isn't installed; adding .torrent links needs it."

if [[ $make_default == true ]]; then
  xdg-mime default io.github.design_nexus.Torrents.desktop x-scheme-handler/magnet application/x-bittorrent
  say "Magnet links and .torrent files now open in Torrents."
fi

case ":$PATH:" in
  *":$bin:"*) ;;
  *) warn "$bin isn't on your PATH; launch Torrents from the app launcher, or add it to PATH." ;;
esac

say "Done. Open Torrents from the app launcher, or run: torrents"
