# Operations on PostgreSQL queries

These contracts carry the public libpg_query operations into Rust. Examples use behaviors recorded in the C fixtures. Version-sensitive outputs, including fingerprint vectors and serialized messages, use the corresponding PostgreSQL 19 compatibility schema and algorithm version.

## Native library

> [spec:pgq:req:api.native]
> All public query operations MUST build from a Rust workspace release
> without compiling or linking libpg_query or a PostgreSQL C backend.
> A release test MUST exercise parsing, codecs, scanning, splitting,
> normalizing, fingerprinting, utility classification, deparsing, summaries
> and PL/pgSQL with both probes absent and no database connection.

## Split statements

> [spec:pgq:req:split.scanner]
> Scanner splitting MUST emit a slice at a top-level semicolon or at end of input when the slice contains a SQL keyword and parentheses are balanced. It MUST preserve lexically valid but grammatically invalid slices such as `SELECT FROM`, skip keyword-free slices, retain intervening trivia at the start of a returned slice and report an unterminated quoted string as a scanner error.

> [spec:pgq:req:split.parser]
> Parser splitting MUST return the byte location and length of each parsed statement and propagate a syntax error if the input fails to parse. For a final raw statement with the zero-length sentinel it MUST use the remaining byte length of the input.

> [spec:pgq:req:split.boundaries]
> Both splitters MUST preserve semicolons inside strings, comments and a parenthesized `CREATE RULE` action list. For `SELECT 1; SELECT 2` the scanner's second slice MUST start at byte 9 with length 9 and the parser's at byte 10 with length 8.

## Normalize

> [spec:pgq:req:normalize.substitutions]
> Full normalization MUST substitute eligible literal spans with numbered parameters and preserve bytes outside those spans. Existing parameters MUST keep their numbers, with new numbers starting above the largest existing one. `SELECT $1, 1` MUST become `SELECT $1, $2`.

> [spec:pgq:req:normalize.literal-spans]
> The normalizer MUST use parser and scanner locations to replace the complete literal, including a folded unary minus and the delimiters of a string. Matching text in a preceding comment MUST remain intact when a later constant supplies the replacement location.

> [spec:pgq:req:normalize.grouping]
> Group and sort ordinals and type modifiers MUST retain their values, including `GROUP BY 1`, `ORDER BY 1` and `varchar(50)`. When a GROUP BY expression corresponds to a target expression, the normalized occurrences MUST reuse the target's parameter numbers as in the grouping fixtures.

> [spec:pgq:req:normalize.utility]
> Utility normalization MUST apply the compatibility operation's utility-specific substitutions. It MUST preserve `SELECT 1` and `CALL fn(1)` verbatim and produce `CREATE ROLE postgres PASSWORD $1` for `CREATE ROLE postgres PASSWORD 'xyz'`.

## Fingerprint

> [spec:pgq:req:fingerprint.digest]
> The operation MUST hash the versioned compatibility token stream with XXH3-64 and its fingerprint-version seed. It MUST return both the unsigned 64-bit value and its zero-padded, lowercase, 16-digit hexadecimal rendering. Golden vectors MUST identify the fingerprint version.

> [spec:pgq:req:fingerprint.constants]
> `SELECT 1`, `SELECT 2` and `SELECT $1` MUST produce the same default fingerprint. Parsing the same source with different supported start modes MUST use the tree from the requested mode.

> [spec:pgq:req:fingerprint.lists]
> Lists named `fromClause`, `targetList`, `cols`, `rexpr`, `valuesLists` and `args` MUST order children by ascending unsigned child digest using the inherited fingerprint options. For each distinct digest the parent MUST receive one representative child's original fingerprint token stream. Other lists, including top-level statements, MUST retain input order. The checked-in libpg_query 18.6 C fixture with fingerprint version 3 gives `330267237da5535f` for both `SELECT x, y FROM z` and `SELECT y, x FROM z`.

> [spec:pgq:req:fingerprint.aliases]
> In a SELECT or DML relation reference the default fingerprint MUST use an alias when present and omit the schema. The ignore-aliases option MUST use the relation name and the include-schema option MUST contribute the schema. The PG17 compatibility option MUST combine both flags.

> [spec:pgq:req:fingerprint.relation-digits]
> By default `SELECT * FROM orders_2024_01` and `SELECT * FROM orders_2024_02` MUST have equal fingerprints. Full-relation-name mode MUST distinguish them and MUST compose with the schema and alias flags.

## Utility classification

> [spec:pgq:req:utility.classify]
> The classifier MUST emit false for select, insert, update, delete and merge raw statement nodes and true for other statement types. A WITH clause MUST take the classification of its enclosing statement.

> [spec:pgq:req:utility.sequence]
> Classification MUST return one flag per nonempty statement, in order. `SELECT 1; SHOW fsync` MUST produce false followed by true. Empty input MUST produce an empty sequence and invalid input MUST return the parse error.

## Deparse

> [spec:pgq:req:deparse.round-trip]
> For supported raw statements, deparsing and reparsing MUST preserve the structural tree after excluding source locations and lengths. This includes precedence in arithmetic, nested joins and parenthesized set operations with their own ORDER BY clauses.

> [spec:pgq:req:deparse.stream]
> Default deparsing MUST render an empty statement list as an empty string. Between consecutive statements it MUST emit a semicolon and space. A trailing semicolon or newline MUST remain absent in default output.

> [spec:pgq:req:deparse.quoting]
> Deparsing MUST preserve the value and identity of literals and identifiers. For example, an identifier containing an embedded double quote MUST round-trip as one identifier, and a string containing a single quote MUST round-trip as the original string.

> [spec:pgq:req:deparse.pretty]
> The pretty printer MUST support indent width, maximum line length for breakable lists, comma placement and a trailing-newline option. The default pretty settings MUST use a four-space indentation, an 80-character list width and end-of-line commas.

> [spec:pgq:req:deparse.comments]
> Comment extraction MUST retain delimiters, text, insertion location and surrounding line-break information. When supplied to deparsing, comments MUST stay in source order and attach within the appropriate statement, including when the input contains multiple statements.

> [spec:pgq:req:deparse.protobuf]
> Deparsing a valid, matching-schema ParseResult MUST render the same SQL as its decoded raw statements. The public entry point MUST return a structured error for undecodable input and for a decoded node missing a child required to render it.

## Summary

> [spec:pgq:req:summary.entities]
> The summary MUST return tables, aliases, CTE names, functions, filter columns and statement types. Each table and function MUST preserve its full name, schema and local name, and the reference context enum (`None`, `Select`, `DML`, `DDL` or `Call`). Each filter-column result MUST carry the column and its table and schema qualifiers when present. For `SELECT s.f(a) FROM s.t WHERE s.t.a = 1`, the result MUST include table `s.t` in Select context, function `s.f` in Call context, the filter components `s`, `t`, `a`, and the statement type `SelectStmt`.

> [spec:pgq:req:summary.cte-scope]
> A read from a visible CTE MUST be recorded as a CTE reference. `WITH t AS (SELECT * FROM s.t) SELECT * FROM t` MUST report physical table `s.t` and CTE `t`. A recursive self-reference MUST remain a CTE reference and a DML target sharing a CTE name MUST remain a physical table.

> [spec:pgq:req:summary.nested-filters]
> A predicate containing a subquery MUST contribute filter columns from each scope. In `SELECT * FROM t WHERE a = (SELECT b FROM u WHERE c = 1)`, filter columns MUST include `a` and `c` and exclude the subquery's projected column `b`.

> [spec:pgq:req:summary.truncation]
> A truncation limit of -1 MUST omit the shortened query. With limit 40, `SELECT a, b, c, d, e, f FROM xyz WHERE a = b` MUST shorten to `SELECT ... FROM xyz WHERE a = b`. With limit 10, `SELECT * FROM t` MUST shorten to `SELECT ...`.

> [spec:pgq:req:summary.protobuf]
> The serialized summary MUST decode to the same entities, statement types and truncation result as the native summary for the same SQL, parse options and limit. A rejected input MUST return its parse diagnostic.

## PL/pgSQL

> [spec:pgq:req:plpgsql.routines]
> The PL/pgSQL entry point MUST locate function, procedure and DO bodies in SQL input and preserve their order in the result. Input consisting solely of ordinary SQL MUST return an empty array. DO bodies with the default language MUST be parsed as PL/pgSQL.

> [spec:pgq:req:plpgsql.datums]
> The JSON function tree MUST preserve the order and references of datums, including parameters, declared variables, implicit variables and record fields. Trigger fixtures using `NEW` and `OLD` MUST preserve referential links from assignments and expressions to the recorded datums where the reference exposes them.

> [spec:pgq:req:plpgsql.body]
> The result MUST retain block nesting, statement order, body line numbers, and the text and raw parse mode of embedded SQL expressions. For trigger assignment `NEW.name = upper(cleanString(NEW.name))`, the embedded expression MUST preserve that text and its two-name assignment mode.

> [spec:pgq:req:plpgsql.errors]
> Outer SQL errors and PL/pgSQL body errors MUST surface through the public error result. A body error MUST retain the context and cursor position when the PL/pgSQL reference supplies them. Parsing a later valid routine MUST be independent of the failure.

## Rust pg_query API

> [spec:pgq:req:api.pg-query+2]
> The pgsql-syntax workspace crate MUST provide the pg_query.rs 18.1.0 query operations on the native PostgreSQL 19 backend. Callers MUST be able to inspect and edit the PostgreSQL 19 protobuf tree, traverse its nodes, and deparse the edited tree. Building the crate MUST use the pinned descriptor without invoking a C compiler or protoc.
> SQL summaries and edited-tree analysis MUST share query-block CTE visibility rules and traverse all expression fields. Each DML target and schema-qualified table reference MUST remain physical. The compatibility parse result MUST snapshot metadata at construction.
> The immutable and mutable visitors MUST reach all protobuf record fields and lend mutable records for one callback. Wire import MUST require the exact PG19 version. Operations on typed trees MUST reject incompatible explicit versions.
> Callers MUST be able to select one grammar mode independently of scanner flags, and obtain SQLSTATE, character cursor and available PL/pgSQL context through structured errors. Existing parser options and legacy error categories MUST remain supported.
