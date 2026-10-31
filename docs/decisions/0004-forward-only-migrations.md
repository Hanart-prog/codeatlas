# 4. Small forward-only migrator, schema added per phase

**Status:** Accepted

**Decision:** use about 100 lines of our own code rather than a migration crate, since the requirements are narrow: ordered SQL files, atomic application, and refusing edited migrations or newer databases. There are no down migrations, because the database is a rebuildable cache.

Phase 0 creates only `repositories` and `analysis_runs`. Later tables arrive with the code that uses them, so their shape reflects real queries. The [specification review](../spec-review.md) has already changed several of them.

**Consequence:** the conceptual schema in the specification is a target, not what `0001` contains.
