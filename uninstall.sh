#!/bin/bash
# Remove Nexus Office.
#
# Options:
#   --purge   also remove its settings, recent files lists and unsaved-work copies
set -euo pipefail

purge=false
[[ ${1:-} == "--purge" ]] && purge=true

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

pkill -x nexus-office 2>/dev/null || true
rm -f "$HOME/.local/bin/nexus-office"
for app in write sheets present; do
  rm -f "$HOME/.local/bin/nexus-$app" "$HOME/.local/share/applications/nexus-$app.desktop"
done
rm -f "$HOME/.local/share/mime/packages/nexus-office.xml"
update-mime-database "$HOME/.local/share/mime" 2>/dev/null || true
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/nexus-office"
say "Removed the apps. Your documents are untouched."

if [[ $purge == true ]]; then
  rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/nexus-office"
  rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/nexus-office"
  rm -rf "${XDG_STATE_HOME:-$HOME/.local/state}/nexus-office"
  say "Removed its settings, recent files lists and unsaved-work copies."
fi
