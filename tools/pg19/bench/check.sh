#!/bin/sh
# Run after pg19-reference import, check-schema and corpus capture.
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
    sh "$repo/tools/pg19/bench/build-release.sh" "$work"
elif [ ! "$rust_bench" -ef "$work/bench-rust" ]; then
    cp -- "$rust_bench" "$work/bench-rust"
fi
make -s -C "$build/src/backend" -f Makefile -f "$repo/tools/pg19/bench/bench.mk" \
    "repo=$repo" "schema=$pgimport/pg19-2-1" "work=$work" "$work/bench-c"
{
    "${RUSTC:-rustc}" -vV
    cargo -V
    if [ "$#" -eq 3 ]; then
        echo 'Rust release: profile-guided, panic=abort, statically linked mimalloc'
        echo 'Profile inputs: test/sql/deparse/*.sql'
    else
        printf 'Rust executable supplied by caller: %s\n' "$rust_bench"
    fi
    awk '/^#define PG_VERSION_STR / || /^#define CONFIGURE_ARGS /' "$build/src/include/pg_config.h"
    awk '/^CFLAGS = /' "$build/src/Makefile.global"
    awk '/^model name[[:space:]]*:/ {print; exit}' /proc/cpuinfo
} > "$work/toolchain.txt"
python3 tools/pg19/bench/prepare.py "$corpus" "$work"
exec python3 tools/pg19/bench/compare.py "$work" "$work/bench-rust"
