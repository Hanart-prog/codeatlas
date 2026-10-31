# 9. Limited tsconfig paths support in v1

**Status:** Accepted

The specification lists `tsconfig` `paths` as post-MVP. Many TypeScript applications import through aliases such as `@/lib/x`, so without support their graphs would be sparse and impact and backtest results would have little static evidence. The project owner accepted the recommendation to include it.

**Decision:** Phase 2 resolves `paths` and `baseUrl` from the root `tsconfig.json`, read from the Git blob at each commit, never from the working tree. `extends` chains and monorepo workspaces stay out of scope and are reported as unresolved.
