#!/bin/bash
# Remove Notepad.
#
# Options:
#   --purge   also remove its settings, open tabs and unsaved text
set -euo pipefail

purge=false
[[ ${1:-} == "--purge" ]] && purge=true

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

pkill -x notepad 2>/dev/null || true
rm -f "$HOME/.local/bin/notepad" \
  "$HOME/.local/share/applications/io.github.design_nexus.Notepad.desktop" \
  "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.design_nexus.Notepad.svg"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
say "Removed the app."

if [[ $purge == true ]]; then
  rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/nexus-notepad" "${XDG_DATA_HOME:-$HOME/.local/share}/nexus-notepad"
  say "Removed its settings, open tabs and unsaved text."
else
  say "Settings and unsaved text are kept. Run with --purge to remove them too."
fi
