summary_includes = -I$(work) -I$(repo) -I$(repo)/src -I$(repo)/src/include -I$(repo)/vendor/upb -I$(repo)/vendor/upb/third_party/utf8_range
summary_flags = -fvisibility=hidden -Wno-declaration-after-statement -Wno-unused-function -ffunction-sections -fdata-sections $(summary_includes)

$(work)/probe.o: $(repo)/tools/pg19/tests/summary-probe.c $(repo)/tools/pg19/tests/summary-probe.mk $(repo)/src/pg_query_summary.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/statements.o: $(work)/statements.c
	$(COMPILE.c) $(summary_flags) -Wno-switch $< -o $@
$(work)/supports.o: $(repo)/src/pg_query_raw_tree_walker_supports.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/upb.o: $(repo)/vendor/upb/upb.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/mini.o: $(repo)/protobuf/pg_query.upb_minitable.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/utf8.o: $(repo)/vendor/upb/third_party/utf8_range/utf8_range.c
	$(COMPILE.c) $(summary_flags) $< -o $@
$(work)/summary-probe: $(OBJS) $(work)/probe.o $(work)/statements.o $(work)/supports.o $(work)/upb.o $(work)/mini.o $(work)/utf8.o
	$(CC) $(CFLAGS) $(call expand_subsys,$^) $(LDFLAGS) -Wl,--wrap=main,--gc-sections $(LIBS) -o $@
