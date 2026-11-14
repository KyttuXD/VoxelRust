#!/bin/sh
# Print the update public key as a PEM file (it is not secret), for the
# website's updater (scripts/site-updater.sh) to check the release list with.
#
#   sh scripts/export-public-key.sh > update-key.pub.pem
#   scp update-key.pub.pem you@server:/etc/voxelrust/update-key.pub.pem
#
# The private key is VOXELRUST_SIGN_KEY (default ~/.voxelrust/update-signing-key.pem);
# only its public half is printed.
set -eu
KEY="${VOXELRUST_SIGN_KEY:-$HOME/.voxelrust/update-signing-key.pem}"
[ -f "$KEY" ] || { echo "no signing key at $KEY" >&2; exit 1; }
openssl pkey -in "$KEY" -pubout
