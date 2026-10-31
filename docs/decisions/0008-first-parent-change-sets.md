# 8. One change set per first-parent commit

**Status:** Accepted

The specification asks for first-parent history and also to exclude merges. In a repository that uses merge commits, the first-parent chain is mostly merges, so excluding them would leave almost nothing.

**Decision:** walk first parents and diff every commit against its first parent, merges included. Positions on this chain are linear, so identities and path validity use positions rather than commit ids. Identities belong to a run, because they depend on the window and on rename settings.

**Consequence:** a merged branch counts once, and one merge can bundle unrelated work. The bulk threshold will filter large ones when co-change arrives. An all-reachable mode is not built.
