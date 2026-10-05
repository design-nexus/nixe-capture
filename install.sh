#!/bin/bash
# Install Fonts, a font manager for Omarchy.
#
# From a clone, ./install.sh builds from source and installs to ~/.local.
# Piped from curl, it fetches the source first.
set -euo pipefail

REPO="design-nexus/nexus-fonts"

for arg in "$@"; do
  case "$arg" in
    -h | --help)
      sed -n '2,5p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m::\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m::\033[0m %s\n' "$*" >&2; exit 1; }

command -v hyprctl >/dev/null || warn "Hyprland wasn't found. Fonts is made for Omarchy (Hyprland), but runs elsewhere too."

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

if command -v pacman >/dev/null; then
  need=()
  command -v cargo >/dev/null || need+=(rust)
  for pkg in gtk4 fontconfig pango tar xz pkgconf gcc; do
    pacman -Qq "$pkg" >/dev/null 2>&1 || need+=("$pkg")
  done
  if ((${#need[@]})); then
    say "Installing ${need[*]}"
    sudo pacman -S --needed --noconfirm "${need[@]}"
  fi
elif ! command -v cargo >/dev/null; then
  die "Rust (cargo) is needed to build Fonts. Install it, plus GTK 4, fontconfig and pango, and run this again."
fi

say "Building Fonts (a few minutes the first time)"
(cd "$src" && cargo build --release --locked)

bin="$HOME/.local/bin"
apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/scalable/apps"
mkdir -p "$bin" "$apps" "$icons"

say "Installing to ~/.local"
install -m 755 "$src/target/release/fonts" "$bin/fonts"
install -m 644 "$src/data/io.github.design_nexus.Fonts.desktop" "$apps/"
install -m 644 "$src/data/io.github.design_nexus.Fonts.svg" "$icons/"
update-desktop-database "$apps" 2>/dev/null || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

case ":$PATH:" in
  *":$bin:"*) ;;
  *) warn "$bin isn't on your PATH; launch Fonts from the app launcher, or add it to PATH." ;;
esac

say "Done. Open Fonts from the app launcher, or run: fonts"
