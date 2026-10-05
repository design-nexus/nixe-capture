#!/bin/bash
# Remove Fonts.
#
# Options:
#   --purge   also remove its settings and every font it installed
set -euo pipefail

purge=false
[[ ${1:-} == "--purge" ]] && purge=true

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

pkill -x fonts 2>/dev/null || true
rm -f "$HOME/.local/bin/fonts" \
  "$HOME/.local/share/applications/io.github.design_nexus.Fonts.desktop" \
  "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.design_nexus.Fonts.svg"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/nexus-fonts"
say "Removed the app. Fonts it installed are still there."

if [[ $purge == true ]]; then
  rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/nexus-fonts" \
    "${XDG_STATE_HOME:-$HOME/.local/state}/nexus-fonts" \
    "${XDG_DATA_HOME:-$HOME/.local/share}/fonts/nexus-fonts"
  fc-cache -f "${XDG_DATA_HOME:-$HOME/.local/share}/fonts" 2>/dev/null || true
  say "Removed its settings and the fonts it installed."
fi
