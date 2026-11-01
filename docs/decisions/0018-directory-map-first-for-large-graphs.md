# 18. Show large graphs as a directory map first

**Status:** Accepted

A force layout of several hundred files is unreadable and slow, and the Explorer's top 500 files hid the rest. Search and the Change Impact file picker only saw those 500, so most of a large repository could not be reached.

**Decision:** above 200 files the Explorer opens on a directory map computed in Rust. Each box is a subdirectory, or the files directly in the current directory, and arrows count imports between boxes. Selecting a box opens it. A directory with a single subdirectory is skipped, so a repository under `packages/` does not open on one box. At most 60 groups are returned by default, and the response counts what was left out. Files stay one click away.

Search moved to the server, through `search` on `/graph`, so it reaches every file.

**Consequence:** map arrows add up imports of every kind, including `import type`. They show structure, not what runs, and two opposite arrows do not mean a cycle. A red border is the only cycle signal, and it comes from the file-level graph.
