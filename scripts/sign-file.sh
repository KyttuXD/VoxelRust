#!/bin/sh
# Sign a file for the updater: writes <file>.sig, an Ed25519 signature over
# the exact bytes of the file (64 raw bytes), and checks it again before
# returning.
#
#   sh scripts/sign-file.sh dist/nightly/nightly.txt
#
# The private key is the PEM file named by VOXELRUST_SIGN_KEY (default
# ~/.voxelrust/update-signing-key.pem; make one with scripts/make-signing-key.sh).
# Without a key this stops with an error and writes nothing: a release the
# game would refuse is never made by accident.
set -eu
file="${1:?usage: sign-file.sh FILE}"
KEY="${VOXELRUST_SIGN_KEY:-$HOME/.voxelrust/update-signing-key.pem}"
if [ ! -f "$KEY" ]; then
  echo "no signing key at $KEY" >&2
  echo "Make one with  sh scripts/make-signing-key.sh  (or point VOXELRUST_SIGN_KEY at your key)." >&2
  exit 1
fi
[ -f "$file" ] || { echo "no such file: $file" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
# OpenSSL 3 wants -rawin for a message that is not a digest; 1.1.1 does not know it.
sign() { openssl pkeyutl -sign -inkey "$KEY" -rawin -in "$file" -out "$tmp/sig" 2>/dev/null || openssl pkeyutl -sign -inkey "$KEY" -in "$file" -out "$tmp/sig"; }
sign
openssl pkey -in "$KEY" -pubout -out "$tmp/pub.pem"
if ! { openssl pkeyutl -verify -pubin -inkey "$tmp/pub.pem" -rawin -in "$file" -sigfile "$tmp/sig" >/dev/null 2>&1 \
       || openssl pkeyutl -verify -pubin -inkey "$tmp/pub.pem" -in "$file" -sigfile "$tmp/sig" >/dev/null 2>&1; }; then
  echo "the signature made for $file does not verify; not writing it" >&2
  exit 1
fi
[ "$(wc -c < "$tmp/sig" | tr -d ' ')" = 64 ] || { echo "the signature is not 64 bytes (is $KEY an Ed25519 key?)" >&2; exit 1; }
cp "$tmp/sig" "$file.sig"
