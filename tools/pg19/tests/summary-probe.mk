libpg_query = $(repo)/tools/pg19/libpg_query
summary_includes = -I$(work) -I$(libpg_query) -I$(libpg_query)/src -I$(libpg_query)/src/include -I$(libpg_query)/vendor/upb -I$(libpg_query)/vendor/upb/third_party/utf8_range
summary_flags = -fvisibility=hidden -Wno-declaration-after-statement -Wno-unused-function -ffunction-sections -fdata-sections $(summary_includes)

$(work)/probe.o: $(repo)/tools/pg19/tests/summary-probe.c $(repo)/tools/pg19/tests/summary-probe.mk $(libpg_query)/src/pg_query_summary.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/statements.o: $(work)/statements.c
	$(COMPILE.c) $(summary_flags) -Wno-switch $< -o $@
$(work)/supports.o: $(libpg_query)/src/pg_query_raw_tree_walker_supports.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/upb.o: $(libpg_query)/vendor/upb/upb.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/mini.o: $(libpg_query)/protobuf/pg_query.upb_minitable.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/utf8.o: $(libpg_query)/vendor/upb/third_party/utf8_range/utf8_range.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/deparse.o: $(work)/deparse.c
	$(COMPILE.c) $(summary_flags) -Wno-shadow -Wno-unused-variable -Wno-switch $< -o $@
$(work)/truncate.o: $(libpg_query)/src/pg_query_summary_truncate.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/summary-probe: $(OBJS) $(work)/probe.o $(work)/statements.o $(work)/supports.o $(work)/upb.o $(work)/mini.o $(work)/utf8.o $(work)/truncate.o $(work)/deparse.o
	$(CC) $(CFLAGS) $(call expand_subsys,$^) $(LDFLAGS) -Wl,--wrap=main,--gc-sections $(LIBS) -o $@
