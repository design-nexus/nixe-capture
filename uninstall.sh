#!/bin/bash
# Remove Nexus PDF.
#
# Options:
#   --purge   also remove its settings, recent files list and saved signatures
set -euo pipefail

APP_ID="io.github.design_nexus.Pdf"
purge=false
[[ ${1:-} == "--purge" ]] && purge=true

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

pkill -x pdf 2>/dev/null || true
rm -f "$HOME/.local/bin/pdf" \
  "$HOME/.local/share/applications/$APP_ID.desktop" \
  "$HOME/.local/share/icons/hicolor/scalable/apps/$APP_ID.svg"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/nexus-pdf"
say "Removed the app. Your PDF files are untouched."

if [[ $purge == true ]]; then
  data="${XDG_DATA_HOME:-$HOME/.local/share}/nexus-pdf"
  rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/nexus-pdf"
  rm -rf "$data"
  say "Removed its settings, recent files list and saved signatures."
fi
