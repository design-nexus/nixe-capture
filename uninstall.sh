#!/bin/bash
# Remove Settings.
#
#   curl -fsSL https://raw.githubusercontent.com/design-nexus/omarchy-settings/main/uninstall.sh | bash
#
# Options:
#   --purge   also remove everything Settings configured: its Hyprland file (and
#             the line that loads it), the equalizer service, and its settings
set -euo pipefail

purge=false
[[ ${1:-} == "--purge" ]] && purge=true

cfg="${XDG_CONFIG_HOME:-$HOME/.config}"
say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }

pkill -x settings 2>/dev/null || true

if command -v omarchy-plugin-remove >/dev/null && [[ -d $cfg/omarchy/plugins/design-nexus.settings-gear ]]; then
  omarchy-plugin-remove design-nexus.settings-gear --yes >/dev/null 2>&1 || true
fi
rm -rf "$cfg/omarchy/plugins/design-nexus.settings-gear"

rm -f "$HOME/.local/bin/settings" \
  "$HOME/.local/share/applications/io.github.design_nexus.Settings.desktop" \
  "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.design_nexus.Settings.svg"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
say "Removed the app."

if [[ $purge == true ]]; then
  if systemctl --user list-unit-files settings-eq.service >/dev/null 2>&1; then
    systemctl --user disable --now settings-eq.service 2>/dev/null || true
  fi
  rm -f "$cfg/systemd/user/settings-eq.service" "$cfg/pipewire/settings-eq.conf"
  systemctl --user daemon-reload 2>/dev/null || true

  if [[ -f $cfg/hypr/hyprland.lua ]]; then
    sed -i '/require("hypr.settings") -- Settings app/d' "$cfg/hypr/hyprland.lua"
  fi
  rm -f "$cfg/hypr/settings.lua"
  rm -rf "$cfg/settings"
  command -v hyprctl >/dev/null && hyprctl reload >/dev/null 2>&1 || true
  say "Removed everything Settings configured."
else
  say "Your settings were kept. Run with --purge to remove them too."
fi
