# 17. Build reports on request

**Status:** Accepted. Replaces `reports/:report_id` in section 12 of the specification.

The specification downloads stored reports, but has no way to create one, and everything in a report can be recomputed from stored analysis.

**Decision:** `GET /repositories/:id/report?format=json|html` builds the report on each request and stores nothing, so there is no storage to expire or fall out of date. `codeatlas report` shares the code.

HTML is rendered in Rust from a typed document. It escapes every value, runs no script, loads nothing, and carries its own `default-src 'none'` policy. The API sends it as a download, so repository text is never rendered as a page of the dashboard's origin. Reports hold the repository's folder name and never its full path, because a path can reveal a user name and reports are made to be shared.

**Consequences:** a backtest appears only if it ran on the reported commit, so all sections describe one state. Generating a report on a repository with uncached timeline commits can take a while, and is subject to the request time limit.
