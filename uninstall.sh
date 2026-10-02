#!/bin/bash
# Remove Nexus Media Player.
#
# Options:
#   --purge   also remove its settings, library database, playlists and cached pictures
set -euo pipefail

APP_ID="io.github.design_nexus.MediaPlayer"
purge=false
[[ ${1:-} == "--purge" ]] && purge=true

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

pkill -x media-player 2>/dev/null || true
rm -f "$HOME/.local/bin/media-player" "$HOME/.local/bin/nexus-media-player" \
  "$HOME/.local/share/applications/$APP_ID.desktop" \
  "$HOME/.local/share/icons/hicolor/scalable/apps/$APP_ID.svg"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/nexus-media-player"
say "Removed the app. Your video files are untouched."

if [[ $purge == true ]]; then
  data="${XDG_DATA_HOME:-$HOME/.local/share}/nexus-media-player"
  rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/nexus-media-player"
  rm -f "$data/library.db" "$data/library.db-wal" "$data/library.db-shm" "$data/state.json"
  rmdir "$data" 2>/dev/null || true
  say "Removed its settings, library database and playlists."
fi
