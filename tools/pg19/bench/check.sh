#!/bin/sh
# Run after pgsql-reference import, check-schema and corpus capture.
set -eu
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    echo "usage: sh check.sh PG19_IMPORT CORPUS OUTPUT [RUST_BENCH]" >&2
    exit 2
fi
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
pgimport=$(CDPATH= cd -- "$1" && pwd -P)
corpus=$(CDPATH= cd -- "$2" && pwd -P)
mkdir -p -- "$3"
work=$(CDPATH= cd -- "$3" && pwd -P)
if [ "$#" -eq 4 ]; then
    rust_bench=$(realpath -- "$4")
    if [ ! -f "$rust_bench" ] || [ ! -x "$rust_bench" ]; then
        echo "Rust benchmark is not an executable file: $4" >&2
        exit 2
    fi
fi
build=$pgimport/patched
optimization=$(awk '/^CFLAGS = / { for (i = 3; i <= NF; i++) if ($i ~ /^-O/) opt = $i; print opt }' "$build/src/Makefile.global")
if [ "$optimization" != "-O3" ]; then
    build=$work/c-build
    CFLAGS=-O3 "$repo/tools/pg19/build-reference.sh" patched "$pgimport/patched-source" "$build"
fi
cd "$repo"
if [ "$#" -eq 3 ]; then
    cargo build --release --locked -p pgsql-parser --example parse-bench --target-dir "$repo/target"
    rust_bench=$repo/target/release/examples/parse-bench
fi
if [ ! "$rust_bench" -ef "$work/bench-rust" ]; then
    cp -- "$rust_bench" "$work/bench-rust"
fi
schema=
for candidate in "$pgimport"/pg19-*; do
    if cmp -s "$candidate/protobuf/inputs.sha256" "$repo/protobuf/pg19/inputs.sha256"; then
        schema=$candidate
        break
    fi
done
if [ -z "$schema" ]; then
    echo "Run 'cargo run -p pgsql-reference -- check-schema $pgimport' to generate the pinned C field writers." >&2
    exit 1
fi
make -s -C "$build/src/backend" -f Makefile -f "$repo/tools/pg19/bench/bench.mk" \
    "repo=$repo" "libpg_query=$pgimport/libpg_query" "schema=$schema" "work=$work" "$work/bench-c"
{
    "${RUSTC:-rustc}" -vV
    cargo -V
    if [ "$#" -eq 3 ]; then
        echo 'Rust build: cargo build --release --locked -p pgsql-parser --example parse-bench'
    else
        printf 'Rust executable supplied by caller: %s\n' "$rust_bench"
    fi
    awk '/^#define PG_VERSION_STR / || /^#define CONFIGURE_ARGS /' "$build/src/include/pg_config.h"
    awk '/^CFLAGS = /' "$build/src/Makefile.global"
    awk '/^model name[[:space:]]*:/ {print; exit}' /proc/cpuinfo
} > "$work/toolchain.txt"
python3 tools/pg19/bench/prepare.py "$corpus" "$work"
exec python3 tools/pg19/bench/compare.py "$work" "$work/bench-rust"
