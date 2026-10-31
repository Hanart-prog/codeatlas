# 1. Keep CodeAtlas data outside analyzed repositories

**Status:** Accepted

The specification allows a `.codeatlas/` directory or the OS cache directory, and also forbids modifying analyzed repositories. A `.codeatlas/` inside a repository would be a write to it.

**Decision:** the database defaults to the OS cache directory (`~/Library/Caches/codeatlas` on macOS, `$XDG_CACHE_HOME/codeatlas` on Linux). `--data-dir`, `CODEATLAS_DATA_DIR` or the config file can override it. Configuration is never read from an analyzed repository, which could otherwise raise its own limits.

**Consequence:** the cache can be deleted at any time and rebuilt.
