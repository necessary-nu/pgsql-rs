# PostgreSQL 19 raw parsing in Rust

PostgreSQL 19.0 supplies the SQL grammar, scanner rules, keyword catalog, and raw node definitions. The stock and libpg_query-compatible reference builds identify which behavior belongs to the release and which comes from a reviewed patch. This contract covers raw parsing through the Rust API.

## Lexer and public token stream

> [spec:pgq:req:lex.keywords]
> The lexer MUST recognize unquoted keywords case-insensitively using the pinned keyword catalog. The scan API MUST expose each keyword's symbolic token and category, distinguishing reserved, unreserved, column-name and type/function-name keywords. A quoted keyword such as `"select"` MUST scan as an identifier.

> [spec:pgq:req:lex.byte-spans]
> Each public scan token MUST carry a zero-based start byte and exclusive end byte in the original input. Slicing those bytes MUST recover the spelling, including surrounding quotes, dollar tags or comment delimiters. In `SELECT 'é', x` the string occupies bytes 7 through 10 inclusive.

> [spec:pgq:req:lex.comments]
> Public scanning MUST emit line and nested block comments in source order and omit whitespace between tokens. SQL parsing MUST consume those comments as trivia. The characters `/*` inside a string and a semicolon inside a comment MUST retain their lexical context.

> [spec:pgq:req:lex.quoted-values]
> Scanning MUST distinguish ordinary, escape, Unicode, bit and hex strings and quoted identifiers using the input prefixes. A tagged dollar string MUST end only at the matching tag. The public scan of `$tag$a;b$tag$` MUST return one string token.

> [spec:pgq:req:lex.numbers-and-parameters]
> The scanner MUST keep the numeric literal category selected by PostgreSQL, including `FCONST` for a decimal integer too large for `ICONST`. It MUST distinguish `$1` from a dollar-quoted string and emit the PostgreSQL token for `::` as a unit.

> [spec:pgq:req:lex.parser-lookahead]
> The grammar-facing token stream MUST apply the pinned lookahead rules across whitespace and comments, preserving the source position of the original token. Cases include `NOT IN`, `NULLS FIRST`, `WITH ORDINALITY`, `WITHOUT TIME` and `FORMAT JSON`. The public token stream MUST retain the original keyword tokens.

> [spec:pgq:req:lex.unicode-escape]
> The grammar-facing scanner MUST decode Unicode identifiers and strings with the default escape or an explicit valid `UESCAPE` character. For example, `U&"d!0061t" UESCAPE '!'` MUST supply the identifier `dat` to the grammar while retaining the location of the opening token.

> [spec:pgq:req:lex.token-results]
> The scan protobuf encoder MUST serialize the native token sequence in order, preserving symbolic identities, keyword categories and both byte positions. On decoding the protobuf for empty or whitespace-only input, the token list MUST be empty.

## Raw tree

> [spec:pgq:req:tree.node-layout]
> The native tree MUST represent the node types, variants, lists, field values and child order produced by PostgreSQL 19 grammar actions. A select target holding the integer 1 MUST retain the chain from `SelectStmt` through `ResTarget` to the integer variant of `A_Const`.

> [spec:pgq:req:tree.literal-variants]
> The raw-tree protobuf codec MUST round-trip SQL NULL, Boolean false, integer zero and the empty string as four distinct native constant variants, including when the selected protobuf variant contains its scalar default.

> [spec:pgq:req:tree.statement-spans]
> Default-mode output MUST contain one ordered `RawStmt` per nonempty statement. Its location MUST identify the start assigned by the grammar, and its length MUST reach the terminating semicolon when present, excluding the delimiter. An unterminated final statement MUST retain PostgreSQL's zero-length sentinel.

> [spec:pgq:req:tree.node-locations]
> Locations recorded by grammar actions MUST be byte offsets in the original SQL, including offsets after multibyte text. Where the grammar assigns an unknown location, the native tree MUST preserve that sentinel separately from offset zero.

## Grammar tables and semantic actions

> [spec:pgq:req:grammar.tables]
> Rust parser tables MUST derive from the pinned grammar with the reviewed libpg_query overlay and preserve its start symbol, token mapping, productions, precedence, associativity and conflict resolutions. Rebuilding tables from identical recorded inputs MUST reproduce the same tables.

> [spec:pgq:req:grammar.action-inventory]
> Each production in the imported grammar MUST map to its Rust reduction action or the applicable default reduction. The action inventory MUST identify the upstream production and source action and report an unmapped or changed action when updating the grammar.

> [spec:pgq:req:grammar.reductions]
> A reduction MUST receive the values and locations of its right-hand symbols in grammar order. Empty, default and midrule actions used by the imported grammar MUST preserve its values and locations. Comparison with the patched reference MUST check the resulting trees as well as acceptance.

> [spec:pgq:req:grammar.raw-sql]
> Default parsing MUST accept or reject raw SQL according to the pinned grammar without a database connection or catalog lookup. For example, a select from an unknown relation MUST produce a relation reference. Syntax errors in a later statement MUST fail the parse of the whole input.

> [spec:pgq:req:grammar.sql-families]
> Grammar verification MUST exercise data queries, set operations, DML including MERGE, DDL, transaction and utility commands. The PG19 corpus MUST include `REPACK t USING INDEX i` and verify the resulting `RepackStmt`, target relation and index name.

> [spec:pgq:req:grammar.expression-binding]
> Operators and set operations MUST produce the nesting prescribed by the PostgreSQL grammar. For `SELECT 1 + 2 * 3` multiplication MUST be the right child of addition. Explicit parentheses MUST override the default binding in `SELECT (1 + 2) * 3`.


> [spec:pgq:sem:grammar.query-actions]
> A query reduction preserves target order, join structure and the nesting of
> set operations. In `SELECT 1 UNION SELECT 2 INTERSECT SELECT 3`, the right
> subtree of the union represents the intersection.

> [spec:pgq:sem:grammar.dml-actions]
> A data modification reduction retains the target relation, ordered source
> expressions, conflict or merge actions and the RETURNING list. PG19's
> `ON CONFLICT (id) DO SELECT RETURNING *` yields an `OnConflictClause` with
> action `ONCONFLICT_SELECT`.

> [spec:pgq:sem:grammar.utility-actions]
> Utility reductions construct the statement-specific node selected by PG19.
> `WAIT FOR LSN '0/16B6C50'` produces a `WaitStmt`; `REPACK t USING INDEX i`
> records the relation and index on a `RepackStmt`. DDL and transaction
> reductions preserve names, ordered options and locations from the grammar.

> [spec:pgq:req:grammar.parse-modes]
> The Rust API MUST expose default SQL, type name, PL/pgSQL expression and the three PL/pgSQL assignment start modes. Type-name mode MUST accept `character varying(32)`; expression mode MUST accept `v IS NULL`; assignment modes MUST preserve the selected name count in `PLAssignStmt` for `v := 1`, `v.x := 1` and `v.x.y := 1`.

> [spec:pgq:req:grammar.string-settings]
> Ordinary string literals MUST follow PG19 standard-conforming behavior. Escape strings MUST use PG19's backslash-quote policy for the current call, defaulting to safe encoding. A successful or failed call with a custom policy MUST leave later and concurrent calls with their own settings.

PostgreSQL 19 fixes standard-conforming strings on and removes the escape-string warning setting. The Rust API's treatment of the two corresponding legacy C option bits is an open compatibility decision.

> [spec:pgq:req:grammar.diagnostics]
> A syntax or scanner failure MUST yield a structured error containing the reference message and its reported cursor position. Positive cursor positions count characters from one, including after multibyte input. An absent position MUST remain distinguishable from the first character. Error-at-EOF and error-at-token cases MUST be comparable separately.

> [spec:pgq:req:grammar.overlay-differences]
> Patched behavior MUST be traceable to a specific overlay and a paired reference case. Cases MUST exercise placeholders in utility commands and a numeric parameter followed by identifier text. For each case, the Rust parser MUST reproduce the patched reference outcome, with the stock outcome recorded for comparison.

> [spec:pgq:req:grammar.corpus-parity]
> Differential verification MUST extend the eight smoke inputs with cases from the pinned PostgreSQL SQL regression suite and the repository's parse, parse-options, scan and split fixtures. Each extracted regression case MUST retain its source path and byte span. The extraction procedure MUST document statement boundaries, psql commands, COPY data and intentionally invalid SQL, accounting for skipped or transformed inputs. Cases MUST cover all six parse modes and compare Rust acceptance, error messages and character positions, public token spans and raw AST fields and locations against the applicable reference. Expected failures MUST be checked as outcomes. Each intentional difference between the stock and patched references MUST have a recorded classification.

> [spec:pgq:req:grammar.runtime]
> The Rust parser MUST implement lexing, table execution and semantic reductions in Rust. Production parsing MUST function with both C reference binaries absent. Independent parse calls MUST return independently usable trees and diagnostics.

## JSON and protobuf

> [spec:pgq:req:codec.json-envelope]
> JSON output MUST include the numeric PostgreSQL release version and an ordered `stmts` array. On default-mode empty input the array MUST be empty. For PG19.0 the reported version MUST be 190000.

> [spec:pgq:req:codec.json-fields]
> JSON nodes MUST use the compatibility field names, node wrappers, symbolic enums and omission of defaults specified by the target serializer. The tree for `SELECT 0` MUST retain an integer-valued constant and the tree for `SELECT NULL` MUST retain its null flag.

> [spec:pgq:req:codec.schema-import]
> The project MUST derive and pin a libpg_query protobuf descriptor for the patched PG19 inputs, using their node layouts, enumerations and scanner token numbers alongside the libpg_query rules for assigning field numbers, node oneofs and API envelopes. The import MUST record input and generator identities and the descriptor digest. An update MUST review changes to existing field numbers and types, node oneof tags, and enum values against the previous descriptor and record the supported schema version and any wire incompatibilities.

> [spec:pgq:req:codec.protobuf-schema]
> Protobuf output MUST decode with the pinned release schema. Node discriminants, numbered fields, repeated order, enum values and constant oneof presence MUST reconstruct the native raw tree. Compare decoded fields, including locations and statement boundaries. An independent protobuf reader using the pinned descriptor MUST decode the Rust output for `REPACK t USING INDEX i` and recover its `RepackStmt`, relation `t` and index name `i`.

> [spec:pgq:req:codec.errors-and-depth]
> Serialization MUST return the parse diagnostic for rejected SQL. Encoding and decoding deeply nested valid trees MUST either complete or return a structured depth error. A conversion failure MUST yield an error result and leave subsequent conversions usable.
