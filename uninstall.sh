#!/bin/bash
# Remove Music.
#
# Options:
#   --purge   also remove its settings, library database, playlists and covers
set -euo pipefail

purge=false
[[ ${1:-} == "--purge" ]] && purge=true

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

pkill -x music 2>/dev/null || true
rm -f "$HOME/.local/bin/music" \
  "$HOME/.local/share/applications/io.github.design_nexus.Music.desktop" \
  "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.design_nexus.Music.svg"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/nexus-music"
say "Removed the app. Your music files are untouched."

if [[ $purge == true ]]; then
  rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/nexus-music" "${XDG_DATA_HOME:-$HOME/.local/share}/nexus-music"
  say "Removed its settings, library database and playlists."
fi
