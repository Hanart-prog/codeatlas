# 16. Carry stop signals on the database session

**Status:** Accepted

Cancellation, query time limits and progress all have to reach loops deep inside the engine. Passing a parameter through every function would change most signatures and about 120 test call sites for something most callers do not use. A thread-local would be silent if work ever moved to another thread.

**Decision:** `Work` holds a cancel flag, a deadline and a progress listener. It is set on the `Database` a task uses, since every long operation already takes one. Each API request gets its own connection and a deadline of `server.query_timeout_seconds` (default 120). Jobs get a cancel flag and a listener instead, with no deadline, because they can be watched and stopped.

The setting is in `ServerConfig`, not `Limits`, because `Limits` feeds the results cache key and a time limit cannot change a result.

**Consequences:**

- Stopping takes effect at the next step: a commit, a file, a backtest case or a timeline point. One huge diff or a slow SQLite query runs to its end first.
- A timed-out request keeps what it finished, such as parsed files and snapshots, so a retry goes further. The error says so.
- A job cancelled while parsing has already stored its history, so it is cancelled with a usable analysis and no graph. The graph is built when next needed.
- Nothing stops work when a browser disconnects. The deadline bounds that case.
