#!/bin/bash
# Install Settings, a control panel for Omarchy.
#
#   curl -fsSL https://raw.githubusercontent.com/design-nexus/omarchy-settings/main/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/design-nexus/omarchy-settings/main/install.sh | bash -s -- --gear
#
# From a clone, ./install.sh builds from source.
#
# Options:
#   --gear     also add a gear button to the Omarchy bar that opens Settings
#   --source   build from source even when a prebuilt release is available
set -euo pipefail

REPO="design-nexus/omarchy-settings"
ASSET="omarchy-settings-x86_64-linux.tar.gz"

gear=false
source_build=false
for arg in "$@"; do
  case "$arg" in
    --gear) gear=true ;;
    --source) source_build=true ;;
    -h | --help)
      sed -n '2,13p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m::\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m::\033[0m %s\n' "$*" >&2; exit 1; }

command -v hyprctl >/dev/null || warn "Hyprland wasn't found. Settings is made for Omarchy (Hyprland)."

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Where the files to install come from: a directory holding the binary at
# ./settings plus ./data and ./plugin.
payload=""

script_dir=""
if [[ -n ${BASH_SOURCE[0]:-} && -f ${BASH_SOURCE[0]} ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi

build_from() {
  local src="$1"
  if ! command -v cargo >/dev/null; then
    if command -v pacman >/dev/null; then
      say "Installing Rust and GTK 4 build dependencies"
      sudo pacman -S --needed --noconfirm rust gtk4 pkgconf gcc
    else
      die "Rust (cargo) is needed to build Settings. Install it and run this again."
    fi
  fi
  say "Building Settings (a minute or two the first time)"
  (cd "$src" && cargo build --release --locked)
  mkdir -p "$work/payload"
  cp "$src/target/release/settings" "$work/payload/settings"
  cp -r "$src/data" "$src/plugin" "$work/payload/"
  payload="$work/payload"
}

if [[ -n $script_dir && -f $script_dir/Cargo.toml ]]; then
  # Running from a clone.
  build_from "$script_dir"
else
  if [[ $source_build == false && $(uname -m) == x86_64 ]]; then
    say "Downloading the latest release"
    url="https://github.com/$REPO/releases/latest/download/$ASSET"
    if curl -fsSL "$url" -o "$work/release.tar.gz" && tar -xzf "$work/release.tar.gz" -C "$work"; then
      payload="$work/omarchy-settings"
    else
      warn "No prebuilt release available; building from source instead."
    fi
  fi
  if [[ -z $payload ]]; then
    command -v git >/dev/null || die "git is needed to fetch the source."
    git clone --depth 1 "https://github.com/$REPO.git" "$work/src"
    build_from "$work/src"
  fi
fi

[[ -x $payload/settings ]] || die "Something went wrong: no settings binary to install."

bin="$HOME/.local/bin"
apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/scalable/apps"
mkdir -p "$bin" "$apps" "$icons"

say "Installing to ~/.local"
install -m 755 "$payload/settings" "$bin/settings"
install -m 644 "$payload/data/io.github.design_nexus.Settings.desktop" "$apps/"
install -m 644 "$payload/data/io.github.design_nexus.Settings.svg" "$icons/"
update-desktop-database "$apps" 2>/dev/null || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

# Refresh the managed Hyprland file if Settings was already in use (the
# volume keys point at this binary). A fresh install touches nothing.
if [[ -f "${XDG_CONFIG_HOME:-$HOME/.config}/settings/state.json" ]]; then
  "$bin/settings" --apply || true
fi

if [[ $gear == true ]]; then
  say "Adding the Settings gear to the bar"
  dest="${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/plugins/design-nexus.settings-gear"
  mkdir -p "$dest"
  install -m 644 "$payload/plugin/manifest.json" "$payload/plugin/BarWidget.qml" "$dest/"
  omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
  omarchy-plugin-enable design-nexus.settings-gear right >/dev/null 2>&1 || warn "Enable it from Settings → Plugins."
fi

if [[ ! -f /usr/lib/lv2/lsp-plugins.lv2/limiter_stereo.ttl ]]; then
  warn "Optional: 'sudo pacman -S lsp-plugins-lv2' adds a limiter, allowing up to +24 dB of preamp without distortion."
fi

case ":$PATH:" in
  *":$bin:"*) ;;
  *) warn "$bin isn't on your PATH; launch Settings from the app launcher, or add it to PATH." ;;
esac

say "Done. Open Settings from the app launcher, or run: settings"
