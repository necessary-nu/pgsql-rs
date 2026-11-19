#include "postgres.h"
#include "pg_query.h"
#include "pg_query_internal.h"
#include "parser/parser.h"
#include "mb/pg_wchar.h"
#include "miscadmin.h"
#include "utils/json.h"

static bool outer_error;
static int last_sqlstate;
static int internal_position;

ErrorData *__real_CopyErrorData(void);
ErrorData *__wrap_CopyErrorData(void);
int __wrap_main(int argc, char **argv);

ErrorData *
__wrap_CopyErrorData(void)
{
	ErrorData *data = __real_CopyErrorData();
	last_sqlstate = data->sqlerrcode;
	internal_position = data->internalpos;
	return data;
}

MemoryContext
pg_query_enter_memory_context(void)
{
	MemoryContext ctx = AllocSetContextCreate(TopMemoryContext, "plpgsql-probe", ALLOCSET_DEFAULT_SIZES);
	MemoryContextSwitchTo(ctx);
	return ctx;
}

void
pg_query_exit_memory_context(MemoryContext ctx)
{
	MemoryContextSwitchTo(TopMemoryContext);
	MemoryContextDelete(ctx);
}

void
pg_query_free_error(PgQueryError *e)
{
	free(e->message);
	free(e->context);
	free(e->filename);
	free(e->funcname);
	free(e);
}

PgQueryInternalParsetreeAndError
pg_query_raw_parse(const char *sql, int options)
{
	MemoryContext context = CurrentMemoryContext;
	PgQueryInternalParsetreeAndError result = {0};

	PG_TRY();
	{
		result.tree = raw_parser(sql, RAW_PARSE_DEFAULT);
	}
	PG_CATCH();
	{
		ErrorData *data;

		MemoryContextSwitchTo(context);
		data = CopyErrorData();
		outer_error = true;
		result.error = calloc(1, sizeof(*result.error));
		result.error->message = strdup(data->message);
		result.error->context = data->context ? strdup(data->context) : NULL;
		result.error->cursorpos = data->cursorpos;
		FlushErrorState();
	}
	PG_END_TRY();
	return result;
}

int
__wrap_main(int argc, char **argv)
{
	if (argc < 2)
		return 2;
	MemoryContextInit();
	set_stack_base();
	SetDatabaseEncoding(PG_UTF8);
	for (int i = 1; i < argc; i++)
	{
		PgQueryPlpgsqlParseResult result;
		StringInfo out;
		if (!pg_verify_mbstr(PG_UTF8, argv[i], strlen(argv[i]), true))
			return 2;
		outer_error = false;
		result = pg_query_parse_plpgsql(argv[i]);
		out = makeStringInfo();
		if (result.error)
		{
			appendStringInfo(out, "{\"functions\":null,\"error\":{\"phase\":\"%s\",\"sqlstate\":", outer_error ? "sql" : "plpgsql");
			escape_json(out, unpack_sql_state(last_sqlstate));
			appendStringInfoString(out, ",\"message\":");
			escape_json(out, result.error->message);
			appendStringInfo(out, ",\"position\":%d,\"internal_position\":%d,\"context\":", result.error->cursorpos, internal_position);
			if (result.error->context)
				escape_json(out, result.error->context);
			else
				appendStringInfoString(out, "null");
			appendStringInfoString(out, "}}");
		}
		else
		{
			/* The writer adds line feeds between routines. Inside strings it
			 * escapes them, so removing literal line feeds yields JSON Lines. */
			appendStringInfoString(out, "{\"functions\":");
			for (const char *p = result.plpgsql_funcs; *p; p++)
				if (*p != '\n')
					appendStringInfoChar(out, *p);
			appendStringInfoString(out, ",\"error\":null}");
		}
		puts(out->data);
		pg_query_free_plpgsql_parse_result(result);
		pfree(out->data);
		pfree(out);
	}
	return 0;
}
