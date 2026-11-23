# PostgreSQL 19.0 raw parser patches

These patches adapt the numbered libpg_query patches from the PostgreSQL 18.6
baseline (`27112f8:patches/`) to the PostgreSQL 19.0 release. Apply the
series in filename order to a fresh source tree, before generating the
parser, scanner or node support files. Run this loop from the repository
root with `pgsrc` pointing to the unpacked release.

```sh
for patch_file in tools/pg19/patches/*.patch; do
    patch -d "$pgsrc" -p1 --batch --forward --fuzz=0 < "$patch_file" || exit 1
done
```

Every hunk matches its recorded line number on the pinned release. When
updating the pin, review any reported offsets and regenerate the patch
against the new sources. The original patch series has no entry numbered 02.

| Original patch | Treatment in the PG19 reference |
| --- | --- |
| 01 · Additional parameter references | Ported. Accepts numbered parameters in role passwords, SET forms, EXTRACT fields and typed literals. The SCHEMA, ROLE and SESSION AUTHORIZATION alternatives assign `VariableSetStmt.location` to the argument's byte offset, following the adjacent PG19 literal actions. |
| 03 · Token end locations | Ported. Tracks the exclusive end byte for identifiers and quoted literals, including tokens assembled across multiple scanner rules. |
| 04 · Comments as tokens | Ported. The scanner can return line comments and complete nested block comments. PG19's `scanner_init` sets `return_comments` to false next to its remaining `backslash_quote` setting. The SQL and PL/pgSQL grammars declare the same two extra scanner tokens. |
| 05 · Default LIMIT option | Ported. Zero-initialized raw select nodes carry `LIMIT_OPTION_DEFAULT`, distinct from explicit `LIMIT_OPTION_COUNT`. The paired executor change handles the enum when compiling the full upstream sources. |
| 06 · AllocSet free-list cleanup | Omitted. This adds a teardown entry point for libpg_query's embedded memory contexts. |
| 07 · PL/pgSQL compile helpers | Omitted. This exports the procedural-language compiler internals used by libpg_query's PL/pgSQL API. |
| 08 · Regression SQL substitutions | Omitted. This edits COPY data and delimited identifiers in PostgreSQL regression fixtures for libpg_query split tests. |
| 09 · Parameter adjacent to identifier | Ported. The scanner can tokenize `$1alias` as a parameter followed by an identifier. PG19's parameter-number range check and numeric-literal junk checks still apply. |
| 10 · Namespace hash-table extraction | Omitted. This suppresses generated hash-table code during libpg_query's C source extraction. |
| 11 · Configurable identifier size | Omitted. This permits overriding `NAMEDATALEN` at compile time. The reference uses PostgreSQL's default value of 64. |
| 12 · Function parameter ACL check | Omitted. This changes catalog permission checks during interpretation of function parameters after raw parsing. |
| 13 · Expression location stack check | Ported. Checks stack depth during recursive expression location lookups used by the raw grammar. |
| 14 · Dotted names | Ported. Accumulates components of configuration and parameter names in a list, then joins them once. |
| 15 · String constant locations | Ported. Preserves byte offsets on role options, foreign options, function and DO options, NOTIFY payloads and subscription connection strings. The PG19 `SERVER name` subscription syntax has no connection string to locate. |
