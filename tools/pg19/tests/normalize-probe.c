#include "pg_query_normalize.c"
#include "lib/stringinfo.h"

int __wrap_main(int argc, char **argv);

MemoryContext
pg_query_enter_memory_context(void)
{
	MemoryContext ctx = AllocSetContextCreate(TopMemoryContext, "normalize probe",
											 ALLOCSET_DEFAULT_SIZES);
	MemoryContextSwitchTo(ctx);
	return ctx;
}

void
pg_query_exit_memory_context(MemoryContext ctx)
{
	MemoryContextSwitchTo(TopMemoryContext);
	MemoryContextDelete(ctx);
}

int
__wrap_main(int argc, char **argv)
{
	StringInfoData sql;
	PgQueryNormalizeResult result;
	char buf[8192];
	size_t count;
	bool failed;

	if (argc != 2 || (strcmp(argv[1], "full") && strcmp(argv[1], "utility")))
		return 2;
	MemoryContextInit();
	set_stack_base();
	SetDatabaseEncoding(PG_UTF8);
	initStringInfo(&sql);
	while ((count = fread(buf, 1, sizeof(buf), stdin)) > 0)
		appendBinaryStringInfo(&sql, buf, count);
	if (ferror(stdin))
		return 2;
	result = pg_query_normalize_ext(sql.data, strcmp(argv[1], "utility") == 0);
	failed = result.error != NULL;
	if (failed)
		fprintf(stderr, "%s\n", result.error->message);
	else
		fputs(result.normalized_query, stdout);
	pg_query_free_normalize_result(result);
	return failed;
}
