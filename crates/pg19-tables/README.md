# PG19 parser tables

The production crate reads Bison's packed LALR action, default reduction and
goto tables directly from little-endian data. Building it requires only Rust.
The tables and symbol names derive from the PostgreSQL 19.0 grammar with the
patches in `tools/pg19/patches`. See `src/postgres/COPYRIGHT` for the license
and upstream copyright notice.

From the repository root, pass the patched-source directory printed by the
PG19 reference importer to check that the artifacts reproduce with Bison 3.8.2.

```sh
python3 tools/pg19/generate-tables.py "$run/patched-source"
```

For a reviewed upgrade, add `--write`. The generator reports removed or
added productions and changed explicit actions. Its XML rule numbering is
converted to Bison's C reduction numbering and cross-checked against the
generated LHS and RHS-length arrays. The artifact records the input grammar's
SHA-256. All byte offsets are independent of the checkout path and host byte
order.

In `data/productions.tsv` the line ordinal is the one-based reduction number.
Columns give the production (including explicit precedence), source line in
patched `gram.y`, and either `default` or the SHA-256 of Bison's translated
action body. The Rust dispatcher should register explicit actions using both
production and digest. `unmapped_actions` enumerates the holes in a supplied
registry. The implicit `$accept` production is included for numbering;
the final state returns `Action::Accept` directly.
