#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
build=$(CDPATH= cd -- "$1/stock" && pwd -P)
out=$2
make -s -j4 -C "$build/src/interfaces/libpq" all
make -s -C "$build/src/fe_utils" psqlscan.o
make -s -C "$build/src/fe_utils" -f Makefile -f - helper="$here/boundaries.c" output="$out" corpus-boundaries <<'MAKE'
.PHONY: corpus-boundaries
corpus-boundaries:
	$(CC) $(LDFLAGS) $(CPPFLAGS) $(CFLAGS) "$(helper)" psqlscan.o $(top_builddir)/src/interfaces/libpq/libpq.a $(top_builddir)/src/common/libpgcommon.a $(top_builddir)/src/common/libpgcommon_shlib.a $(top_builddir)/src/port/libpgport.a $(LIBS) -o "$(output)"
MAKE
