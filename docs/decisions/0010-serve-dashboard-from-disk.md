# 10. Serve the dashboard from disk

**Status:** Accepted

The specification review proposed embedding the built frontend in the release binary. That makes `cargo build` depend on a prior frontend build, which breaks the rule that the Rust engine works without React.

**Decision:** `codeatlas serve` serves `--web-dir`, defaulting to `apps/web/dist` when it exists, and otherwise serves only the API. A named directory without `index.html` is an error. Responses carry a Content Security Policy.

**Consequence:** the Rust build has no frontend dependency. The dashboard needs `make build` and a repository checkout, which fits this clone-and-run project.
