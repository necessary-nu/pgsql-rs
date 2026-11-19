/* Use the private native summary and wire encoder from the library, with the
 * configured PG19 raw parser. One SQL input per invocation. */
#include "pg_query_summary.c"
#include "mb/pg_wchar.h"
#include "miscadmin.h"
#include "utils/json.h"

int __wrap_main(int argc, char **argv);

int
__wrap_main(int argc, char **argv)
{
	StringInfoData sql;
	char buf[8192];
	size_t n;
	if (argc != 2) return 2;
	MemoryContextInit();
	set_stack_base();
	SetDatabaseEncoding(PG_UTF8);
	initStringInfo(&sql);
	while ((n = fread(buf, 1, sizeof(buf), stdin)) > 0)
		appendBinaryStringInfo(&sql, buf, n);
	if (ferror(stdin)) return 2;
	PG_TRY();
	{
		Node *tree = (Node *) raw_parser(sql.data, (RawParseMode) atoi(argv[1]));
		Summary summary = {0};
		PgQuerySummaryParseResult result = {0};
		pg_query_summary_walk(&summary, tree);
		pg_query_summary_statement_walk(&summary, tree);
		summary_to_protobuf(&result, &summary);
		fwrite(result.summary.data, 1, result.summary.len, stdout);
		free(result.summary.data);
	}
	PG_CATCH();
	{
		MemoryContextSwitchTo(TopMemoryContext);
		ErrorData *e = CopyErrorData();
		StringInfo out = makeStringInfo();
		appendStringInfoString(out, "{\"message\":");
		escape_json(out, e->message);
		appendStringInfo(out, ",\"sqlstate\":\"%s\",\"position\":%d}", unpack_sql_state(e->sqlerrcode), e->cursorpos);
		fputs(out->data, stderr);
		return 1;
	}
	PG_END_TRY();
	return 0;
}
