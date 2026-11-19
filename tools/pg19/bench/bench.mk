$(work)/bench.o: $(repo)/tools/pg19/bench/bench.c force-bench-relink
	$(COMPILE.c) $< -o $@
$(work)/writer.o: $(repo)/src/pg_query_outfuncs_json.c force-bench-relink
	$(COMPILE.c) -DPG_QUERY_OUTFUNCS_H -Wno-unused-function -I$(schema)/src/include -I$(repo)/src/include $< -o $@
$(work)/bench-c: $(OBJS) $(work)/bench.o $(work)/writer.o
	$(CC) $(CFLAGS) $(call expand_subsys,$^) $(LDFLAGS) -Wl,--wrap=main $(LIBS) -o $@

.PHONY: force-bench-relink
force-bench-relink:
