#!/bin/bash
# Install Nexus PDF, a PDF viewer and editor for Omarchy.
#
# From a clone, ./install.sh builds from source and installs to ~/.local.
#
# Options:
#   --default   also make it the default app for PDF files
set -euo pipefail

REPO="design-nexus/nexus-pdf"
APP_ID="io.github.design_nexus.Pdf"

make_default=false
for arg in "$@"; do
  case "$arg" in
    --default) make_default=true ;;
    -h | --help)
      sed -n '2,7p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m::\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m::\033[0m %s\n' "$*" >&2; exit 1; }

command -v hyprctl >/dev/null || warn "Hyprland wasn't found. Nexus PDF is made for Omarchy (Hyprland), but runs elsewhere too."

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
  for pkg in gtk4 poppler-glib pkgconf gcc; do
    pacman -Qq "$pkg" >/dev/null 2>&1 || need+=("$pkg")
  done
  if ((${#need[@]})); then
    say "Installing ${need[*]}"
    sudo pacman -S --needed --noconfirm "${need[@]}"
  fi
elif ! command -v cargo >/dev/null; then
  die "Rust (cargo) is needed to build Nexus PDF. Install it, plus GTK 4 and poppler-glib, and run this again."
fi

say "Building Nexus PDF (a few minutes the first time)"
(cd "$src" && cargo build --release --locked)

bin="$HOME/.local/bin"
apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/scalable/apps"
mkdir -p "$bin" "$apps" "$icons"

say "Installing to ~/.local"
# Replace whatever is there (a file or a link), never write through a link.
rm -f "$bin/pdf"
install -m 755 "$src/target/release/pdf" "$bin/pdf"
install -m 644 "$src/data/$APP_ID.desktop" "$apps/"
install -m 644 "$src/data/$APP_ID.svg" "$icons/"
update-desktop-database "$apps" 2>/dev/null || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

if [[ $make_default == true ]]; then
  types=$(sed -n 's/^MimeType=//p' "$src/data/$APP_ID.desktop" | tr ';' ' ')
  # shellcheck disable=SC2086
  xdg-mime default "$APP_ID.desktop" $types && say "Nexus PDF now opens PDF files."
fi

case ":$PATH:" in
  *":$bin:"*) ;;
  *) warn "$bin isn't on your PATH; launch Nexus PDF from the app launcher, or add it to PATH." ;;
esac

say "Done. Open Nexus PDF from the app launcher, or run: pdf"
