#!/bin/sh
# Make the key pair that signs updates, once.
#
#   sh scripts/make-signing-key.sh            make a key (refuses to replace one)
#   sh scripts/make-signing-key.sh --show     print the public key of the existing one
#
# The private key goes to $VOXELRUST_SIGN_KEY (default
# ~/.voxelrust/update-signing-key.pem), outside the repository, readable by you
# only. NEVER commit it, and back it up: whoever holds it decides what every
# copy of the game installs, and if it is lost, nobody can update the games
# already out (they would need the new program by hand once).
#
# The script prints the public key, which belongs in the game
# (src/client/update_key.rs, `EMBEDDED`), as hex and as a Rust array to paste.
set -eu
KEY="${VOXELRUST_SIGN_KEY:-$HOME/.voxelrust/update-signing-key.pem}"
if [ "${1:-}" != "--show" ]; then
  if [ -e "$KEY" ]; then
    echo "there is already a key at $KEY; not replacing it." >&2
    echo "(Use --show to print its public key. To start over, move it away yourself first:" >&2
    echo " games built with the old public key will not accept updates signed by a new one.)" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$KEY")"
  chmod 700 "$(dirname "$KEY")" 2>/dev/null || true
  (umask 077; openssl genpkey -algorithm ED25519 -out "$KEY")
  chmod 600 "$KEY"
  echo "made $KEY"
elif [ ! -f "$KEY" ]; then
  echo "no key at $KEY" >&2
  exit 1
fi
# A public Ed25519 key in DER ends in its 32 raw bytes.
hex="$(openssl pkey -in "$KEY" -pubout -outform DER | tail -c 32 | od -An -v -tx1 | tr -d ' \n')"
echo
echo "public key (hex):  $hex"
echo
echo "for src/client/update_key.rs:"
echo "const EMBEDDED: [u8; 32] = ["
echo "$hex" | sed 's/../0x&, /g' | fold -w 96 | sed 's/^/    /; s/ *$//'
echo "];"
echo
echo "Then: keep $KEY safe and private (never commit it), rebuild the game, and"
echo "publish with scripts/package-release.sh, which signs nightly.txt with it."
