# PostgreSQL 19 raw parser references

The reference command imports PostgreSQL 19.0 into a development build
directory. The original libpg_query snapshot at `27112f8` supplies the
compatibility behavior and the tests for the 18.6 baseline. Both executables
in a PG19 comparison use the same PostgreSQL release.

> [spec:pgq:req:import.release]
> The importer MUST fetch the official PostgreSQL 19.0 bzip2 source archive
> or accept a local copy of the same archive.

> [spec:pgq:req:import.checksum]
> Before extraction, the importer MUST verify the archive against SHA-256
> `a89fd70efbad6a08c7dbed8d4872e3af30170d9e5ca9dc7906523be95b4e692d`.
> Verification failure MUST stop the import.

> [spec:pgq:req:import.isolation]
> Each import MUST reserve a fresh working directory. Concurrent imports
> and retries after an interrupted build MUST use independent trees.

> [spec:pgq:req:reference.overlay]
> Only the patched source tree receives the reviewed PG19 patch series.
> Patches MUST apply in filename order with fuzz disabled. A failed hunk
> MUST stop the build.

> [spec:pgq:req:reference.build]
> The importer MUST build one executable per flavor from separately unpacked
> source trees. A probe MUST accept SQL without an installed database cluster.

> [spec:pgq:req:corpus.compare]
> For each SQL fixture, the checker MUST run both probes on the same input
> bytes and compare each output with its own checked-in expected result.
> A missing snapshot, probe failure or output difference MUST fail the check.
