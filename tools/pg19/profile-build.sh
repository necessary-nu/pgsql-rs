#!/bin/sh
set -eu
profile_flag=$1
shift
# Cargo's unit separator preserves a profile path containing spaces as one rustc argument.
separator=$(printf '\037')
flags=${CARGO_ENCODED_RUSTFLAGS:-}
if [ "${CARGO_ENCODED_RUSTFLAGS+x}" != x ]; then
    set -f
    for flag in ${RUSTFLAGS:-}; do
        flags="${flags:+$flags$separator}$flag"
    done
fi
export CARGO_ENCODED_RUSTFLAGS="${flags:+$flags$separator}$profile_flag"
exec cargo build "$@"
