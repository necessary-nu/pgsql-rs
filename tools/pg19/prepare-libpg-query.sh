#!/bin/sh
# Materialize the compatibility reference in an import, including in shallow or exported workspaces.
set -eu
if [ "$#" -ne 1 ]; then
    echo "usage: $0 IMPORT_DIR" >&2
    exit 2
fi
tools=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
run=$(CDPATH= cd -- "$1" && pwd -P)
revision=27112f8af5de70368319a12c346c2fb37cd74476
destination=$run/libpg_query
if [ -e "$destination" ]; then
    echo "Reference source already exists: $destination" >&2
    exit 1
fi
source=${LIBPG_QUERY_REPOSITORY:-}
if [ -z "$source" ]; then
    if git -C "$tools" cat-file -e "$revision^{commit}" 2>/dev/null; then
        source=$(git -C "$tools" rev-parse --show-toplevel)
    else
        source=https://github.com/pganalyze/libpg_query.git
    fi
elif [ -d "$source" ]; then
    source=$(CDPATH= cd -- "$source" && pwd -P)
fi
tmp=$(mktemp -d "$run/.libpg_query.XXXXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
git init --quiet "$tmp/source"
git -C "$tmp/source" fetch --quiet --no-tags --depth=1 -- "$source" "$revision"
git -C "$tmp/source" -c core.autocrlf=false checkout --quiet --detach "$revision"
git -C "$tmp/source" apply --index "$tools/libpg_query.patch"
mv -T -- "$tmp/source" "$destination"
