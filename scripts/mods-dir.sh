# Sourced by the other scripts, from the repository's root: finds the folder
# of mods (VOXELRUST_MODS_DIR, default ../VoxelRust-Mods beside the
# repository), makes it absolute and exports it for build.rs.
# Sets MODS. `need_mods` stops the script when the folder has no SHIPPED
# list, for the scripts that package the mods with the game.
MODS="${VOXELRUST_MODS_DIR:-../VoxelRust-Mods}"
if [ -d "$MODS" ]; then MODS="$(cd "$MODS" && pwd)"; fi
export VOXELRUST_MODS_DIR="$MODS"
need_mods() {
  if [ ! -f "$MODS/SHIPPED" ]; then
    echo "no mods folder at $MODS (set VOXELRUST_MODS_DIR to the VoxelRust-Mods folder)" >&2
    exit 1
  fi
}
