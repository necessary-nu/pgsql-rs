pgq_fingerprint_includes = -I$(work)/src/include -I$(repo)/src/include -I$(repo)/src -I$(repo)/vendor -I$(repo)
$(work)/fingerprint-probe.o: $(repo)/tools/pg19/tests/fingerprint-probe.c $(repo)/src/pg_query_fingerprint.c $(repo)/tools/pg19/tests/fingerprint-probe.mk $(work)/src/include/pg_query_fingerprint_defs.c $(work)/src/include/pg_query_fingerprint_conds.c $(work)/src/include/pg_query_enum_defs.c
	$(COMPILE.c) -fvisibility=hidden -ffunction-sections -Wno-unused-function $(pgq_fingerprint_includes) $< -o $@

$(work)/xxhash.o: $(repo)/vendor/xxhash/xxhash.c $(repo)/vendor/xxhash/xxhash.h
	$(COMPILE.c) $< -o $@

$(work)/fingerprint-probe: $(OBJS) $(work)/fingerprint-probe.o $(work)/xxhash.o
	$(CC) $(CFLAGS) $(call expand_subsys,$^) $(LDFLAGS) -Wl,--wrap=main,--gc-sections $(LIBS) -o $@
