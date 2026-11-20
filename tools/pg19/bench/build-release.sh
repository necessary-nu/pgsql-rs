#!/bin/sh
set -eu
if [ "$#" -ne 1 ]; then
    echo "usage: sh build-release.sh OUTPUT" >&2
    exit 2
fi
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
mkdir -p -- "$1"
out=$(CDPATH= cd -- "$1" && pwd -P)
work=$(mktemp -d "$out/pg19-release.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
trap 'exit 1' HUP INT TERM

cd "$repo"
for query in test/sql/deparse/*.sql; do
    cat "$query"
    printf '\0'
done > "$work/train.sql0"
export CARGO_PROFILE_RELEASE_PANIC=abort PG19_BENCH_MIMALLOC=1
sh tools/pg19/train-profile.sh "$work/train.sql0" "$work/parser.profdata"

# Cargo tracks the profile path in RUSTFLAGS. A different set of counts must
# select a different path even when the Rust source and compiler are unchanged.
digest=$(sha256sum < "$work/parser.profdata" | cut -d ' ' -f 1)
profile=$out/parser-$digest.profdata
if ! cmp -s -- "$work/parser.profdata" "$profile"; then
    mv -f -- "$work/parser.profdata" "$profile"
fi
host=$("${RUSTC:-rustc}" -vV | awk '$1 == "host:" {print $2}')
target=${CARGO_TARGET_DIR:-$repo/target}/pg19-profiled/$digest
sh tools/pg19/profile-build.sh "-Cprofile-use=$profile" \
    --locked --release --target "$host" --target-dir "$target" \
    -p pg19-query --example parse-bench --features bench-mimalloc
cp -- "$target/$host/release/examples/parse-bench" "$work/bench-rust"
mv -f -- "$work/bench-rust" "$out/bench-rust"
