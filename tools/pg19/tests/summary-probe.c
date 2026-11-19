/* Use the private native summary and wire encoder from the library, with the
 * configured PG19 raw parser. One SQL input per invocation. */
#include "pg_query_summary.c"
#include "mb/pg_wchar.h"
#include "parser/parser.h"
#include "miscadmin.h"
#include "utils/json.h"

int __wrap_main(int argc, char **argv);

int
__wrap_main(int argc, char **argv)
{
	StringInfoData sql;
	char buf[8192];
	size_t n;
	if (argc < 2 || argc > 5) return 2;
	MemoryContextInit();
	set_stack_base();
	SetDatabaseEncoding(PG_UTF8);
	initStringInfo(&sql);
	while ((n = fread(buf, 1, sizeof(buf), stdin)) > 0)
		appendBinaryStringInfo(&sql, buf, n);
	if (ferror(stdin)) return 2;
	PG_TRY();
	{
		if (argc >= 4 && (atoi(argv[3]) & PG_QUERY_DISABLE_BACKSLASH_QUOTE))
			backslash_quote = BACKSLASH_QUOTE_OFF;
		Node *tree = (Node *) raw_parser(sql.data, (RawParseMode) atoi(argv[1]));
		Summary summary = {0};
		PgQuerySummaryParseResult result = {0};
		pg_query_summary_walk(&summary, tree);
		pg_query_summary_statement_walk(&summary, tree);
		if (argc >= 3 && atoi(argv[2]) != -1)
			pg_query_summary_truncate(&summary, tree, atoi(argv[2]));
		if (argc == 5 && strcmp(argv[4], "text") == 0)
			fputs(summary.truncated_query ? summary.truncated_query : "", stdout);
		else
		{
			summary_to_protobuf(&result, &summary);
			/* Map entry order is unspecified in the library wire result. */
			upb_Arena *a = upb_Arena_New();
			pg_query_SummaryResult *decoded = pg_query_SummaryResult_parse(result.summary.data, result.summary.len, a);
			size_t size = 0;
			if (!decoded) return 2;
			char *bytes = pg_query_SummaryResult_serialize_ex(decoded, kUpb_EncodeOption_Deterministic, a, &size);
			fwrite(bytes, 1, size, stdout);
			upb_Arena_Free(a);
			free(result.summary.data);
		}
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
