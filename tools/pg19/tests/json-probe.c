#include "postgres.h"
#include "mb/pg_wchar.h"
#include "miscadmin.h"
#include "parser/parser.h"
#include "utils/memutils.h"

extern char *pg_query_nodes_to_json(const void *obj);
int __wrap_main(int argc, char **argv);

int
__wrap_main(int argc, char **argv)
{
	if (argc != 2)
		return 2;
	MemoryContextInit();
	set_stack_base();
	SetDatabaseEncoding(PG_UTF8);
	puts(pg_query_nodes_to_json(raw_parser(argv[1], RAW_PARSE_DEFAULT)));
	return 0;
}
