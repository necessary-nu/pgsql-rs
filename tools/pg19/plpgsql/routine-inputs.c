#include "postgres.h"

#include "parser/parser.h"
#include "utils/json.h"
#include "routine-inputs.h"

static void
type_input(StringInfo out, TypeName *type)
{
	ListCell *lc;
	bool first = true;

	if (type == NULL)
	{
		appendStringInfoString(out, "null");
		return;
	}
	appendStringInfoString(out, "{\"names\":[");
	foreach(lc, type->names)
	{
		if (!first)
			appendStringInfoChar(out, ',');
		escape_json(out, strVal(lfirst(lc)));
		first = false;
	}
	appendStringInfo(out, "],\"oid\":%u,\"setof\":%s,\"pct\":%s,\"bounds\":%d}",
					 type->typeOid, type->setof ? "true" : "false",
					 type->pct_type ? "true" : "false", list_length(type->arrayBounds));
}

static void
routine_input(StringInfo out, Node *node)
{
	bool func = IsA(node, CreateFunctionStmt);
	List *opts = func ? castNode(CreateFunctionStmt, node)->options : castNode(DoStmt, node)->args;
	ListCell *lc;
	const char *language = "plpgsql";
	const char *body = NULL;

	appendStringInfoChar(out, '{');
	if (func)
	{
		CreateFunctionStmt *f = castNode(CreateFunctionStmt, node);
		bool first = true;

		appendStringInfo(out, "\"kind\":\"%s\",\"name\":",
						 f->is_procedure ? "procedure" : "function");
		escape_json(out, strVal(linitial(f->funcname)));
		appendStringInfoString(out, ",\"ret\":");
		type_input(out, f->returnType);
		appendStringInfoString(out, ",\"params\":[");
		foreach(lc, f->parameters)
		{
			FunctionParameter *p = lfirst_node(FunctionParameter, lc);

			if (!first)
				appendStringInfoChar(out, ',');
			appendStringInfo(out, "{\"mode\":%d,\"name\":", p->mode);
			escape_json(out, p->name ? p->name : "");
			appendStringInfoString(out, ",\"type\":");
			type_input(out, p->argType);
			appendStringInfo(out, ",\"default\":%s}", p->defexpr == NULL ? "null" :
							 IsA(p->defexpr, A_Const) ? "\"constant\"" : "\"expression\"");
			first = false;
		}
		appendStringInfo(out, "],\"sql_body\":%s,", f->sql_body == NULL ? "false" : "true");
	}
	else
		appendStringInfoString(out, "\"kind\":\"do\",");
	foreach(lc, opts)
	{
		DefElem *d = lfirst_node(DefElem, lc);
		if (strcmp(d->defname, "language") == 0)
			language = strVal(d->arg);
		else if (strcmp(d->defname, "as") == 0)
			body = func ? strVal(llast(castNode(List, d->arg))) : strVal(d->arg);
	}
	appendStringInfoString(out, "\"lang\":");
	escape_json(out, language);
	appendStringInfoString(out, ",\"body\":");
	if (body)
		escape_json(out, body);
	else
		appendStringInfoString(out, "null");
	appendStringInfoChar(out, '}');
}

void
routine_inputs(const char *sql, bool typename_only)
{
	MemoryContext context = CurrentMemoryContext;
	StringInfo out = makeStringInfo();

	PG_TRY();
	{
		List *parsed = raw_parser(sql, typename_only ? RAW_PARSE_TYPE_NAME : RAW_PARSE_DEFAULT);
		ListCell *lc;
		bool first = true;

		if (typename_only)
			type_input(out, linitial_node(TypeName, parsed));
		else
		{
			appendStringInfoChar(out, '[');
			foreach(lc, parsed)
			{
				Node *stmt = lfirst_node(RawStmt, lc)->stmt;
				if (!IsA(stmt, DoStmt) && !IsA(stmt, CreateFunctionStmt))
					continue;
				if (!first)
					appendStringInfoChar(out, ',');
				routine_input(out, stmt);
				first = false;
			}
			appendStringInfoChar(out, ']');
		}
	}
	PG_CATCH();
	{
		ErrorData *e;
		MemoryContextSwitchTo(context);
		e = CopyErrorData();
		resetStringInfo(out);
		appendStringInfoString(out, "{\"message\":");
		escape_json(out, e->message);
		appendStringInfo(out, ",\"position\":%d,\"sqlstate\":", e->cursorpos);
		escape_json(out, unpack_sql_state(e->sqlerrcode));
		appendStringInfoChar(out, '}');
		FlushErrorState();
	}
	PG_END_TRY();
	puts(out->data);
}
