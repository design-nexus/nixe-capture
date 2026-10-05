#!/bin/bash
# Install Nexus Office (Write, Sheets and Present) for Omarchy.
#
# From a clone, ./install.sh builds from source and installs to ~/.local.
# Piped from curl, it fetches the source first.
#
# Options:
#   --default   also make the apps the defaults for their file types
set -euo pipefail

REPO="design-nexus/nexus-office-suite"
# The apps this build ships.
APPS=(write sheets present)

make_default=false
for arg in "$@"; do
  case "$arg" in
    --default) make_default=true ;;
    -h | --help)
      sed -n '2,8p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m::\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m::\033[0m %s\n' "$*" >&2; exit 1; }

command -v hyprctl >/dev/null || warn "Hyprland wasn't found. Nexus Office is made for Omarchy (Hyprland), but runs elsewhere too."

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

script_dir=""
if [[ -n ${BASH_SOURCE[0]:-} && -f ${BASH_SOURCE[0]} ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi

src="$script_dir"
if [[ -z $src || ! -f $src/Cargo.toml ]]; then
  command -v git >/dev/null || die "git is needed to fetch the source."
  git clone --depth 1 "https://github.com/$REPO.git" "$work/src"
  src="$work/src"
fi

if command -v pacman >/dev/null; then
  need=()
  command -v cargo >/dev/null || need+=(rust)
  for pkg in gtk4 gtksourceview5 libspelling hunspell hunspell-en_us pkgconf gcc; do
    pacman -Qq "$pkg" >/dev/null 2>&1 || need+=("$pkg")
  done
  if ((${#need[@]})); then
    say "Installing ${need[*]}"
    sudo pacman -S --needed --noconfirm "${need[@]}"
  fi
elif ! command -v cargo >/dev/null; then
  die "Rust (cargo) is needed to build Nexus Office. Install it, plus GTK 4, GtkSourceView 5 and libspelling, and run this again."
fi

say "Building Nexus Office (a few minutes the first time)"
(cd "$src" && cargo build --release --locked)

bin="$HOME/.local/bin"
apps="$HOME/.local/share/applications"
mime="$HOME/.local/share/mime"
mkdir -p "$bin" "$apps" "$mime/packages"

say "Installing to ~/.local"
# Replace whatever is there (a file or a link), never write through a link.
rm -f "$bin/nexus-office"
install -m 755 "$src/target/release/nexus-office" "$bin/nexus-office"
for app in "${APPS[@]}"; do
  rm -f "$bin/nexus-$app"
  install -m 755 "$src/packaging/nexus-$app" "$bin/nexus-$app"
  install -m 644 "$src/packaging/nexus-$app.desktop" "$apps/"
done
install -m 644 "$src/packaging/nexus-office.xml" "$mime/packages/"
update-mime-database "$mime" 2>/dev/null || true
update-desktop-database "$apps" 2>/dev/null || true

if [[ $make_default == true ]]; then
  for app in "${APPS[@]}"; do
    types=$(sed -n 's/^MimeType=//p' "$src/packaging/nexus-$app.desktop" | tr ';' ' ')
    # shellcheck disable=SC2086
    xdg-mime default "nexus-$app.desktop" $types
  done
  say "Nexus Office now opens its file types."
fi

case ":$PATH:" in
  *":$bin:"*) ;;
  *) warn "$bin isn't on your PATH; open the apps from the app launcher, or add it to PATH." ;;
esac

say "Done. Open Nexus Write, Sheets or Present from the app launcher, or run: nexus-write, nexus-sheets, nexus-present"
