#!/bin/sh
# Write the list the updater reads, for the bare programs in a folder.
#
#   sh scripts/write-nightly-list.sh <folder> <commit> <seq>
#
# Writes <folder>/nightly.txt (the commit, the release number (`seq`), and one
# line per bare program: build name, file name, SHA-256 and size) and <folder>/nightly.txt.sig, its Ed25519 signature made with the key
# in VOXELRUST_SIGN_KEY (see scripts/sign-file.sh). Used by the
# release script and by scripts/test-update.sh, so the two cannot drift.
set -eu
dir="$1"; commit="$2"; seq="$3"
{
  echo "commit $commit"
  echo "seq $seq"
  for pair in linux-x86_64:voxelrust-linux-x86_64.bin windows-x86_64:voxelrust-windows-x86_64.exe \
              macos-x86_64:voxelrust-macos-x86_64.bin macos-legacy-x86_64:voxelrust-macos-legacy-x86_64.bin; do
    build="${pair%%:*}"; file="${pair##*:}"
    [ -f "$dir/$file" ] || continue
    echo "$build $file $(sha256sum "$dir/$file" | cut -d' ' -f1) $(wc -c < "$dir/$file" | tr -d ' ')"
  done
} > "$dir/nightly.txt"
# Sign it: the game reads nightly.txt only with a good nightly.txt.sig beside
# it. Stops here (leaving no signature) if there is no signing key.
sh "$(dirname "$0")/sign-file.sh" "$dir/nightly.txt"
