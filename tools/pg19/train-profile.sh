#!/bin/sh
# Produce a rustc profile for the native raw parser from NUL-delimited SQL.
set -eu
if [ "$#" -ne 2 ]; then
    echo "usage: sh train-profile.sh TRAINING.sql0 OUTPUT.profdata" >&2
    exit 2
fi
if [ ! -s "$1" ]; then
    echo "The training input must contain NUL-delimited SQL." >&2
    exit 2
fi
input=$(CDPATH= cd -- "$(dirname -- "$1")" && pwd -P)/$(basename -- "$1")
mkdir -p -- "$(dirname -- "$2")"
output_dir=$(CDPATH= cd -- "$(dirname -- "$2")" && pwd -P)
output=$output_dir/$(basename -- "$2")
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
cd "$repo"
compiler=${RUSTC:-rustc}
host=$("$compiler" -vV | awk '$1 == "host:" {print $2}')
profdata=${LLVM_PROFDATA:-$("$compiler" --print sysroot)/lib/rustlib/$host/bin/llvm-profdata}
if ! command -v "$profdata" >/dev/null 2>&1; then
    echo "Install llvm-tools-preview for the active Rust toolchain or set LLVM_PROFDATA to its llvm-profdata." >&2
    exit 1
fi
work=$(mktemp -d "$output_dir/pg19-training.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
trap 'exit 1' HUP INT TERM
target=${CARGO_TARGET_DIR:-$repo/target}/pg19-instrumented
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Cprofile-generate" \
    cargo build --locked --release --target "$host" --target-dir "$target" \
    -p pg19-query --example parse-bench
LLVM_PROFILE_FILE="$work/%m-%p.profraw" \
    "$target/$host/release/examples/parse-bench" parse "$input" 4
"$profdata" merge -o "$work/merged.profdata" "$work"/*.profraw
mv -f -- "$work/merged.profdata" "$output"
