# 6. Declare the full CLI, and fail loudly for what is not built

**Status:** Accepted. Every command and section is now built, and the rule stands for anything added later.

**Decision:** all commands from the specification are declared, so `--help` shows the target interface. Each unbuilt command is marked in its help text, writes nothing to stdout, exits 1 and names its phase. `serve <repo>` is rejected rather than ignored, since silently dropping the argument would look like success. Dashboard sections follow the same rule.

Exit codes: 0 success, 1 failure, 2 usage error.
