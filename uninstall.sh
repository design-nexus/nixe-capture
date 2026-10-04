#!/bin/bash
# Remove Nexus Archive.
#
# Options:
#   --purge   also remove its settings and recent archives list
set -euo pipefail

APP_ID="io.github.design_nexus.Archive"
purge=false
[[ ${1:-} == "--purge" ]] && purge=true

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

pkill -x archive 2>/dev/null || true
rm -f "$HOME/.local/bin/archive" \
  "$HOME/.local/share/applications/$APP_ID.desktop" \
  "$HOME/.local/share/icons/hicolor/scalable/apps/$APP_ID.svg"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/nexus-archive"
say "Removed the app. Your archives are untouched."

if [[ $purge == true ]]; then
  rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/nexus-archive" "${XDG_DATA_HOME:-$HOME/.local/share}/nexus-archive"
  say "Removed its settings and recent archives list."
fi
