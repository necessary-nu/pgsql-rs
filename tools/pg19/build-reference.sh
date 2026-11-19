#!/bin/sh
# Build a reference against a freshly extracted (optionally patched) 19.0 tree.
set -eu

if [ "$#" != 3 ]; then
    echo "usage: $0 {stock|patched} SOURCE_DIR BUILD_DIR" >&2
    exit 2
fi
case "$1" in
    stock) patched=0 ;;
    patched) patched=1 ;;
    *) echo "unknown reference flavor: $1" >&2; exit 2 ;;
esac
tool_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
source_dir=$(CDPATH= cd -- "$2" && pwd -P)
mkdir -p -- "$3"
cd -- "$3"
if [ "$source_dir" = "$(pwd -P)" ]; then
    echo "the source and build directories must be distinct" >&2
    exit 2
fi
export LC_ALL=C
# Keep configure tests, compiler temporaries and make jobserver FIFOs private
# even when concurrent builds use separate PID namespaces on a shared /tmp.
TMPDIR="$(pwd -P)/tmp"
export TMPDIR
mkdir -p -- "$TMPDIR"
jobs=${PG19_JOBS:-8}
CFLAGS=${CFLAGS:--O0} "$source_dir/configure" --quiet --without-readline --without-zlib --without-icu

# Finish the headers and support libraries before entering the parallel backend
# build. Both the backend and its timezone subdirectory depend on these.
make -s -j"$jobs" -C src/backend generated-headers
make -s -j"$jobs" -C src/backend submake-libpgport

# Bison assigns keyword numbers after the lexer tokens, including the extra
# comment tokens in the patched grammar. Derive names from each build's enum.
awk '$1 ~ /^[A-Za-z_][A-Za-z_0-9]*$/ && $2 == "=" && $3 ~ /^[0-9]+,?$/ {
    printf "[%s] = \"%s\",\n", $1, $1
}' src/backend/parser/gram.h > src/backend/probe-token-names.inc
cp -- "$tool_dir/probe.c" src/backend/pg19-probe.c

# Reuse PostgreSQL's complete backend object list and configured linker flags.
# GNU ld's --wrap redirects the C runtime's call to main to the probe.
make -s -j"$jobs" -C src/backend -f Makefile -f - PG19_PATCHED="$patched" ../../probe <<'MAKE'
pg19-probe.o: pg19-probe.c probe-token-names.inc parser/gram.h
	$(COMPILE.c) -Iparser -DPG19_PATCHED=$(PG19_PATCHED) $< -o $@

../../probe: $(OBJS) pg19-probe.o
	$(CC) $(CFLAGS) $(call expand_subsys,$^) $(LDFLAGS) -Wl,--wrap=main $(LIBS) -o $@
MAKE
