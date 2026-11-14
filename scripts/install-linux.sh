#!/bin/sh
# Installs VoxelRust for the current user: the program in ~/.local/bin, an icon,
# and a menu entry so it shows up in the app search. No root needed.
# Usage: scripts/install-linux.sh [path-to-voxelrust-binary]
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
bin=${1:-$root/target/release/voxelrust}
[ -f "$bin" ] || { echo "no binary at $bin (cargo build --release first)"; exit 1; }
prefix=${XDG_DATA_HOME:-$HOME/.local/share}
mkdir -p "$HOME/.local/bin" "$prefix/applications" "$prefix/icons/hicolor/256x256/apps"
install -m 755 "$bin" "$HOME/.local/bin/voxelrust"
install -m 644 "$root/assets/icon.png" "$prefix/icons/hicolor/256x256/apps/voxelrust.png"
cat > "$prefix/applications/voxelrust.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=VoxelRust
GenericName=Voxel sandbox game
Comment=A voxel sandbox game written in Rust
Exec=$HOME/.local/bin/voxelrust
Icon=voxelrust
Terminal=false
Categories=Game;AdventureGame;
Keywords=voxel;minecraft;sandbox;blocks;
StartupWMClass=voxelrust
DESK
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$prefix/icons/hicolor" 2>/dev/null || true
echo "Installed: ~/.local/bin/voxelrust and a VoxelRust menu entry."
