# 14. Score from smoothed co-change

**Status:** Accepted. Replaces the scoring in section 8 of the specification.

The specified score, `0.6 * static + 0.4 * history` with at least 3 shared commits, lost to ranking files by how often they change. Backtesting showed most inputs are too young to meet that support, and that imports alone predict co-edits weakly.

**Decision:** score files as `(shared + rate) / (selected + 1) + 0.05 / distance`, where `rate` is the file's share of eligible commits. `min_shared_commits` no longer filters rankings. It now marks evidence as weak, and still filters Explorer edges. Files with no import or shared commit can appear, and carry the `activity_only` evidence kind and a caveat.

The formula was selected on development repositories, then frozen and scored on held-out repositories named in advance ([evaluation](../evaluation.md)).

**Consequence:** results fill the list even with no real evidence, which could read as confidence. Activity-only results are therefore labelled, and a note says when nothing links to the change. Any later change to the formula needs new held-out data, because these repositories have now been seen.
