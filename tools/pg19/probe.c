/* A single-process raw parser and scanner for the PostgreSQL 19.0 reference. */
#include "postgres.h"

#include "lib/stringinfo.h"
#include "mb/pg_wchar.h"
#include "miscadmin.h"
#include "parser/parser.h"
#include "parser/scanner.h"
#include "gram.h"
#include "utils/json.h"
#include "utils/memutils.h"

#if PG_VERSION_NUM != 190000
#error "The reference probe requires PostgreSQL 19.0"
#endif

/* Build this table from the Bison header for the selected grammar. */
static const char *const token_names[] = {
#include "probe-token-names.inc"
};

static void
append_error(StringInfo out)
{
	ErrorData  *error;

	MemoryContextSwitchTo(TopMemoryContext);
	error = CopyErrorData();
	FlushErrorState();
	appendStringInfoString(out, "{\"sqlstate\":");
	escape_json(out, unpack_sql_state(error->sqlerrcode));
	appendStringInfoString(out, ",\"message\":");
	escape_json(out, error->message);
	appendStringInfo(out, ",\"position\":%d}", error->cursorpos);
	FreeErrorData(error);
}

static void
append_parse_result(StringInfo out, const char *sql)
{
	appendStringInfoString(out, "\"tree\":");
	PG_TRY();
	{
		List	   *tree = raw_parser(sql, RAW_PARSE_DEFAULT);

		escape_json(out, nodeToStringWithLocations(tree));
		appendStringInfoString(out, ",\"parse_error\":null");
	}
	PG_CATCH();
	{
		appendStringInfoString(out, "null,\"parse_error\":");
		append_error(out);
	}
	PG_END_TRY();
}

static void
append_tokens(StringInfo out, const char *sql)
{
	core_yy_extra_type extra;
	core_yyscan_t scanner = scanner_init(sql, &extra, &ScanKeywords, ScanKeywordTokens);
	core_YYSTYPE value;
	YYLTYPE		start;
	int			id;
	bool		first = true;

#if PG19_PATCHED
	extra.return_comments = true;
#endif
	while ((id = core_yylex(&value, &start, scanner)) != 0)
	{
		char		punctuation[] = {(char) id, '\0'};
		const char *name = id < 256 ? punctuation : token_names[id];
		int			end;

		/* Flex temporarily terminates the matched input at the token end.
		 * PostgreSQL's own query jumbling code uses the same convention. */
		end = start + strlen(extra.scanbuf + start);
#if PG19_PATCHED
		/* Exercise the exact end offsets exposed by the compatibility patch. */
		if (id == IDENT || id == UIDENT || id == SCONST || id == USCONST ||
			id == BCONST || id == XCONST || id == C_COMMENT)
			end = extra.yyllocend;
#endif
		if (!first)
			appendStringInfoChar(out, ',');
		first = false;
		appendStringInfo(out, "{\"id\":%d,\"name\":", id);
		escape_json(out, name);
		appendStringInfo(out, ",\"start\":%d,\"end\":%d}", start, end);
	}
	scanner_finish(scanner);
}

/* The Linux linker redirects the backend entry point to this probe. */
int __wrap_main(int argc, char **argv);

int
__wrap_main(int argc, char **argv)
{
	StringInfo	out;

	if (argc != 2)
	{
		fprintf(stderr, "usage: %s 'SQL'\n", argv[0]);
		return 2;
	}

	MemoryContextInit();
	set_stack_base();
	SetDatabaseEncoding(PG_UTF8);
	if (!pg_verify_mbstr(PG_UTF8, argv[1], strlen(argv[1]), true))
	{
		fputs("probe input must be UTF-8\n", stderr);
		return 2;
	}
	out = makeStringInfo();
	appendStringInfoChar(out, '{');
	append_parse_result(out, argv[1]);
	appendStringInfoString(out, ",\"tokens\":[");
	PG_TRY();
	{
		append_tokens(out, argv[1]);
		appendStringInfoString(out, "],\"scan_error\":null");
	}
	PG_CATCH();
	{
		appendStringInfoString(out, "],\"scan_error\":");
		append_error(out);
	}
	PG_END_TRY();
	appendStringInfoChar(out, '}');
	puts(out->data);
	return 0;
}
