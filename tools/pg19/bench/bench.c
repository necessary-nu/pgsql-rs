#include "postgres.h"
#include "mb/pg_wchar.h"
#include "miscadmin.h"
#include "parser/parser.h"
#include "utils/memutils.h"
#include <time.h>

extern char *pg_query_nodes_to_json(const void *obj);
int __wrap_main(int argc, char **argv);

static uint64 nanos(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64)ts.tv_sec * 1000000000 + ts.tv_nsec;
}

static size_t run(char **queries, size_t count, size_t calls, bool json, bool dump)
{
    size_t total = 0, index = 0;
    for (size_t i = 0; i < calls; i++)
    {
        MemoryContext ctx = AllocSetContextCreate(TopMemoryContext, "benchmark", ALLOCSET_DEFAULT_SIZES);
        List *tree;
        char *owned = NULL;
        MemoryContextSwitchTo(ctx);
        tree = raw_parser(queries[index], RAW_PARSE_DEFAULT);
        total += list_length(tree);
        if (json)
        {
            char *result = pg_query_nodes_to_json(tree);
            owned = strdup(result);
            if (!owned) abort();
            pfree(result);
        }
        MemoryContextSwitchTo(TopMemoryContext);
        MemoryContextDelete(ctx);
        if (owned)
        {
            __asm__ volatile ("" : : "g" (owned) : "memory");
            if (dump) puts(owned);
            free(owned);
        }
        if (++index == count) index = 0;
    }
    return total;
}

int __wrap_main(int argc, char **argv)
{
    FILE *input;
    char *data, **queries;
    long length;
    size_t count = 0, index = 0, repeats, calls, result;
    bool dump, json;
    uint64 start, elapsed;
    if (argc != 4) return 2;
    input = fopen(argv[2], "rb");
    if (!input || fseek(input, 0, SEEK_END)) return 2;
    length = ftell(input);
    if (length < 1 || fseek(input, 0, SEEK_SET)) return 2;
    data = malloc(length);
    if (!data || fread(data, 1, length, input) != (size_t)length || data[length-1]) return 2;
    fclose(input);
    for (long i = 0; i < length; i++) if (!data[i]) count++;
    queries = malloc(count * sizeof(*queries));
    if (!queries) return 2;
    for (long i = 0; i < length; i++) if (!i || !data[i-1]) queries[index++] = data + i;
    repeats = strtoull(argv[3], NULL, 10);
    if (index != count || !repeats) return 2;
    calls = count * repeats;
    dump = !strcmp(argv[1], "verify");
    json = dump || !strcmp(argv[1], "json");
    if (!(json || !strcmp(argv[1], "parse"))) return 2;
    MemoryContextInit();
    set_stack_base();
    SetDatabaseEncoding(PG_UTF8);
    if (dump) { run(queries, count, count, true, true); return 0; }
    run(queries, count, Max((size_t)1, calls/10), json, false);
    start = nanos();
    result = run(queries, count, calls, json, false);
    elapsed = nanos() - start;
    printf("%" PRIu64 " %zu %zu\n", elapsed, calls, result);
    free(queries);
    free(data);
    return 0;
}
