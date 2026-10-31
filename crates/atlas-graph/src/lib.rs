//! Directed dependency graphs and their algorithms.
//!
//! Phase 0 reserves this crate boundary; graph algorithms arrive in Phase 2.
//! Edge convention, shared by engine, API, docs and UI: `A -> B` means file `A`
//! imports file `B`, so reverse traversal from `B` finds what `B` may impact.
