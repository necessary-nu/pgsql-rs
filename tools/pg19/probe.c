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
append_parse_result(StringInfo out, const char *sql, RawParseMode mode)
{
	appendStringInfoString(out, "\"tree\":");
	PG_TRY();
	{
		List	   *tree = raw_parser(sql, mode);

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

/* Each invocation owns its backend globals. Both scanner_init calls copy
 * backslash_quote, including the scanner created inside raw_parser. */
static bool
read_options(int argc, char **argv, RawParseMode *mode)
{
	static const struct
	{
		const char *option;
		const char *name;
		int			value;
	} choices[] = {
		{"--mode", "default", RAW_PARSE_DEFAULT},
		{"--mode", "type-name", RAW_PARSE_TYPE_NAME},
		{"--mode", "plpgsql-expr", RAW_PARSE_PLPGSQL_EXPR},
		{"--mode", "plpgsql-assign1", RAW_PARSE_PLPGSQL_ASSIGN1},
		{"--mode", "plpgsql-assign2", RAW_PARSE_PLPGSQL_ASSIGN2},
		{"--mode", "plpgsql-assign3", RAW_PARSE_PLPGSQL_ASSIGN3},
		{"--backslash-quote", "off", BACKSLASH_QUOTE_OFF},
		{"--backslash-quote", "on", BACKSLASH_QUOTE_ON},
		{"--backslash-quote", "safe_encoding", BACKSLASH_QUOTE_SAFE_ENCODING},
		{"--backslash-quote", "safe", BACKSLASH_QUOTE_SAFE_ENCODING},
	};

	if (argc < 2 || argc % 2 != 0)
		return false;
	for (int i = 2; i < argc; i += 2)
	{
		size_t		j;

		for (j = 0; j < lengthof(choices); j++)
			if (strcmp(argv[i], choices[j].option) == 0 &&
				strcmp(argv[i + 1], choices[j].name) == 0)
				break;
		if (j == lengthof(choices))
			return false;
		if (strcmp(argv[i], "--mode") == 0)
			*mode = (RawParseMode) choices[j].value;
		else
			backslash_quote = choices[j].value;
	}
	return true;
}

/* The Linux linker redirects the backend entry point to this probe. */
int __wrap_main(int argc, char **argv);

int
__wrap_main(int argc, char **argv)
{
	StringInfo	out;
	RawParseMode mode = RAW_PARSE_DEFAULT;

	if (!read_options(argc, argv, &mode))
	{
		fprintf(stderr, "usage: %s 'SQL' [--mode MODE] [--backslash-quote POLICY]\n", argv[0]);
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
	append_parse_result(out, argv[1], mode);
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
