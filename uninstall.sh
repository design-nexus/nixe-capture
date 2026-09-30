#!/bin/bash
# Remove Torrents.
#
# Options:
#   --purge   also remove its settings, categories and session (your downloaded
#             files are never touched)
set -euo pipefail

purge=false
[[ ${1:-} == "--purge" ]] && purge=true

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

# Let a running instance save its resume data first.
if pgrep -x torrents >/dev/null; then
  torrents --quit 2>/dev/null || true
  for _ in $(seq 1 30); do pgrep -x torrents >/dev/null || break; sleep 0.5; done
  pkill -x torrents 2>/dev/null || true
fi
rm -f "$HOME/.local/bin/torrents" \
  "$HOME/.local/share/applications/io.github.design_nexus.Torrents.desktop" \
  "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.design_nexus.Torrents.svg"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/torrents"
say "Removed the app."

if [[ $purge == true ]]; then
  rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/torrents" "${XDG_DATA_HOME:-$HOME/.local/share}/torrents"
  say "Removed its settings and session. Your downloads are where they were."
fi
