# 12. Combine tree and history renames in comparisons

**Status:** Accepted

Section 9 of the specification asks for best-effort rename resolution between two commits. A diff of the two trees works for any pair but misses a file that drifted past the similarity threshold over many commits. Replaying file identities along the first-parent chain follows that drift, but only relates commits on one line and costs a diff per commit.

**Decision:** use both. The tree diff runs for every pair, and identities are replayed when one commit is on the other's first-parent chain within 2,000 commits. Direct evidence wins any conflict, and each rename records how it was found. Base paths are carried through renames before imports and cycles are compared. Nothing is stored, because snapshots are already cached and the result reproduces identically.

**Consequence:** related commits get better identity, at the cost of up to 2,000 commit diffs per comparison. Diverged commits get the tree diff alone, with a note. A file rewritten past the threshold within one commit still shows as a removal and an addition, because Git records nothing that connects them. Copies are not detected, because a copied file is new to the graph.
