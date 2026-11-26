#!/usr/bin/env bash
# Builds and installs the Cosmnia panel applet for the current user.
# Usage: ./install.sh            install to ~/.local
#        ./install.sh uninstall  remove it again
set -euo pipefail
cd "$(dirname "$0")"

PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor"
ID=io.github.phoscoder.Cosmnia

if [[ "${1:-}" == "uninstall" ]]; then
  pkill -x cosmnia 2>/dev/null || true
  rm -f "$BIN/cosmnia" "$APPS/$ID.desktop" "$ICONS/symbolic/apps/$ID-symbolic.svg"
  echo "Uninstalled Cosmnia."
  exit 0
fi

cargo build --release
install -Dm755 target/release/cosmnia "$BIN/cosmnia"
install -Dm644 res/icons/$ID-symbolic.svg "$ICONS/symbolic/apps/$ID-symbolic.svg"
# Absolute Exec path: cosmic-panel's PATH may not include ~/.local/bin.
mkdir -p "$APPS"
sed "s|^Exec=cosmnia$|Exec=$BIN/cosmnia|" res/desktop/$ID.desktop > "$APPS/$ID.desktop"
# Restart a running instance so the panel picks up the new binary.
pkill -x cosmnia 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -qtf "$ICONS" || true
command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true

echo
echo "Installed Cosmnia."
echo "  • Add it: Settings → Desktop → Panel (or Dock) → Configure applets → Add 'Cosmnia'"
