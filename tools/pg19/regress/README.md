# PostgreSQL 19.0 raw parser regression corpus

This corpus contains 54,471 independently parseable or intentionally invalid
inputs. The count includes 54,374 cases from the 245 files in PostgreSQL's
`src/test/regress/sql/*.sql`, 81 repository C fixture cases, and 16 smoke
and parse-mode cases. Both probes ran on every case. Stock PG19 rejected
209 inputs and the patched build rejected 191. The release tarball, source
inventory, patch series, probe source, per-flavor outcomes and classifications
are pinned in [the reference summary](pg19.0-summary.json).

## Reproduce and compare

Start with a [completed reference import](../README.md), including its stock
and patched build directories. Use Python 3.10 or newer and the compiler,
Flex and GNU make used for the import. The extraction command builds a small
executable against the stock PG19 psql lexer and libpq in that run. Set
`run` to its directory and `out` to a directory dedicated to this capture.

```sh
python3 tools/pg19/regress/extract.py "$run" "$out"
python3 tools/pg19/regress/probes.py capture "$run" "$out" --jobs 4
python3 tools/pg19/regress/report.py "$out" --check
python3 tools/pg19/regress/probes.py compare /absolute/path/to/candidate "$out" --jobs 4
```

The candidate executable receives a single SQL argument followed by
`--mode MODE --backslash-quote POLICY` and emits the reference probe JSON.
By default the comparator checks the entire patched result for exact
equality, including the raw tree with locations, the parse diagnostic, the
token list with IDs, names and byte spans, and the scan diagnostic.
`--reference stock` selects the other outcome. Mismatches print the case ID
and fields that differed and make the command fail. A probe crash or timeout
also fails the run. The checked-in summary pins the two reference builds.
The native PostgreSQL 19.0 release parser matched all 54,471 patched
outcomes, including 54,280 accepted trees and 191 parse errors. Use the
native corpus test below to repeat the comparison after a grammar change.

## Case records and provenance

* `cases.jsonl` stores IDs, source family and path, half-open byte ranges,
  original SQL, parse mode and quote setting. Case IDs consist of the
  family, source path and offsets. For upstream and smoke cases, bytes
  `[start:end]` decode to exactly `sql`. The original leading comments,
  whitespace and terminating semicolon stay with the case.
* `skips.jsonl` records each omitted region with the same source coordinates,
  a reason and the original byte digest. Cases and skipped upstream regions
  partition every source byte. `sources.json` pins each input file by path.
* `outcomes.jsonl.gz` holds one object per case in inventory order, with its
  ID and both complete probe responses. `differences.jsonl` lists the
  responsible overlay features for each differing pair. The summary's two
  outcome digests cover every case ID and full response in that order.
* Repository C arrays are read in their declared row order. A case spans
  the original C string literal(s), including the quotes. The SQL value
  includes C escape decoding and adjacent literal concatenation. Each row
  records its source array and original option bits. SQL from the old
  `error_tests` array must give a parse error. Two parse-options fixtures
  request legacy string settings removed in PG19 and appear in the skip
  ledger with their SQL and option bits. All six PG19 modes are exercised.

## Statement boundaries

The stock PostgreSQL 19.0 psql lexer locates command-ending semicolons,
respecting quoted strings, dollar quotes, comments, parentheses, and
`CREATE FUNCTION` or `CREATE PROCEDURE` bodies containing `BEGIN ATOMIC`
and nested `CASE … END`. The final unterminated fragment is submitted as
an input, including when it contains a syntax error. Catalog and execution
failures in the upstream regression suite may still yield valid raw trees.

For a psql send command such as `\g`, `\gx` or `\gset`, the preceding SQL
is retained and the meta-command through the physical line ending is
recorded separately. Other meta-commands and any pending query fragment
that depends on them are recorded as skipped. No shell commands, variable
substitution, file includes, conditional execution or server-generated SQL
are executed. Independently delimited statements in conditional branches
are exercised as syntax. Unresolved variables and psql escapes that rewrite
SQL have distinct skip reasons.

For `COPY … FROM STDIN` the command is kept. Its line tail and data, including
the `\.` marker, have separate spans. Multiple inline COPY payloads and
`\copy … from stdin` use the same terminator handling. The file
`collate.windows.win1252.sql` contains non-UTF-8 bytes and is recorded as a
whole-file omission. Inputs containing NUL would be recorded likewise.

## Reviewing changes

Run `report.py` without `--check` to write a candidate `summary.json`.
Inspect the case IDs in the difference ledger and the underlying paired
outcomes before updating a pin. The current capture has 14 acceptance
changes for additional parameter references and four for parameters adjacent
to identifiers. The other observed differences are comment tokens, shifted
token numbers, explicit LIMIT enum values, and string location fields.
Classification never changes the stored responses or the candidate
comparison. Unexpected differences receive an `unreviewed-*` label for
investigation.

The focused test exercises extraction boundaries, repository string decoding,
and positive and negative probe comparisons. It also runs from the
reference crate's full import integration test.

```sh
python3 tools/pg19/regress/test_corpus.py "$run"
```

## Measuring native action coverage

The parser example runs the checked-in Rust actions, lexer and native tree
writer. Build the example and run it against the saved patched outcomes.

```sh
cargo build --release --locked -p pg19-parser --example pg19-native
python3 tools/pg19/regress/probes.py coverage target/release/examples/pg19-native "$out"
```

Each unmapped action is listed with its PG19 production, source line, action
digest, number of affected cases and an example case ID. Only the first missing
action in each case is observable. The command compares scanner output for
every case, including cases that reach an unmapped action. For completed
parses it also compares the complete tree or diagnostic. Exit status 0 means
all outcomes matched, 1 reports differences and 2 reports incomplete action
coverage. To inspect one case, pass its SQL and probe flags to the example.

To verify the saved source and outcome digests as well as the native parser
in one test, run this from the repository root after building the release
example. Both environment variables accept absolute paths.

```sh
PG19_REGRESSION_DIR="$out" PG19_NATIVE_PROBE="$PWD/target/release/examples/pg19-native" \
  cargo test --offline --locked -p pg19-parser --test corpus -- --ignored
```
