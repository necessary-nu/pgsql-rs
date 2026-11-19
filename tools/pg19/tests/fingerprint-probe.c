/* The fingerprint token walker and option-aware context are private to this file. */
#include "pg_query_fingerprint.c"
#include "lib/stringinfo.h"
#include "mb/pg_wchar.h"
#include "miscadmin.h"

int __wrap_main(int argc, char **argv);

int
__wrap_main(int argc, char **argv)
{
	FingerprintContext ctx;
	StringInfoData sql;
	char buf[8192];
	size_t count;

	if (argc != 3)
		return 2;
	MemoryContextInit();
	set_stack_base();
	SetDatabaseEncoding(PG_UTF8);
	initStringInfo(&sql);
	while ((count = fread(buf, 1, sizeof(buf), stdin)) > 0)
		appendBinaryStringInfo(&sql, buf, count);
	if (ferror(stdin))
		return 2;
	_fingerprintInitContext(&ctx, NULL, atoi(argv[1]), false);
	_fingerprintNode(&ctx, raw_parser(sql.data, (RawParseMode) atoi(argv[2])), NULL, NULL, 0);
	printf("%016" PRIx64 "\n", (uint64_t) XXH3_64bits_digest(ctx.xxh_state));
	_fingerprintFreeContext(&ctx);
	return 0;
}
