#include "postgres.h"
#include "plpgsql.h"

/* The embedded entry point calls the compiler directly with a synthetic
 * pg_proc tuple. In the server this callback is private to pl_comp.c. */
extern void plpgsql_compile_callback(FunctionCallInfo fcinfo, HeapTuple procTup,
									 const CachedFunctionHashKey *hashkey,
									 CachedFunction *cfunc, bool forValidator);
