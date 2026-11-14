#!/usr/bin/env bash
# Run only the tests of the modules that changed (a quick check while coding).
#
#   scripts/test-changed.sh            # files changed against HEAD (staged or not)
#   scripts/test-changed.sh main       # files changed against another commit
#   scripts/test-changed.sh --full     # everything (what CI runs; takes about 7 s once built)
#
# A changed src/a/b.rs (or src/a/b/mod.rs) runs the tests whose path holds `a::b`.
# A change outside src/ (Cargo.toml, build.rs, data/) runs everything, as does a
# change to a file with no module of its own (src/lib.rs).
set -euo pipefail
cd "$(dirname "$0")/.."
TC="${TOOLCHAIN:-+1.99.1}"
if [ "${1:-}" = "--full" ]; then
    exec cargo "$TC" test --lib
fi
BASE="${1:-HEAD}"
files=$( (git diff --name-only "$BASE"; git ls-files --others --exclude-standard) | sort -u)
filters=()
for f in $files; do
    case "$f" in
        src/lib.rs|src/main.rs|Cargo.toml|Cargo.lock|build.rs|data/*) echo "$f changed: running everything"; exec cargo "$TC" test --lib ;;
        src/*.rs)
            m="${f#src/}"; m="${m%.rs}"; m="${m%/mod}"; m="${m//\//::}"
            filters+=("$m") ;;
        docs/*|*.md|scripts/*|.github/*) ;;
    esac
done
if [ ${#filters[@]} -eq 0 ]; then
    echo "no source file changed: nothing to test"
    exit 0
fi
echo "testing: ${filters[*]}"
status=0
for m in "${filters[@]}"; do
    cargo "$TC" test --lib -q "$m" || status=$?
done
exit $status
