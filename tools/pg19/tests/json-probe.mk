# Included after the patched PostgreSQL backend's configured Makefile.
$(work)/json-probe.o: $(repo)/tools/pg19/tests/json-probe.c
	$(COMPILE.c) $< -o $@

# The JSON writer uses only PostgreSQL types. Its public libpg_query header
# includes version macros for the separately maintained C library.
$(work)/json-writer.o: $(repo)/src/pg_query_outfuncs_json.c $(repo)/src/include/pg_query_json_helper.c $(addprefix $(work)/src/include/,pg_query_outfuncs_defs.c pg_query_outfuncs_conds.c pg_query_enum_defs.c)
	$(COMPILE.c) -DPG_QUERY_OUTFUNCS_H -Wno-unused-function -I$(work)/src/include -I$(repo)/src/include $< -o $@

$(work)/json-probe: $(OBJS) $(work)/json-writer.o $(work)/json-probe.o
	$(CC) $(CFLAGS) $(call expand_subsys,$^) $(LDFLAGS) -Wl,--wrap=main $(LIBS) -o $@
