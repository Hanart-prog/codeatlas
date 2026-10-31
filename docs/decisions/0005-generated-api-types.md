# 5. Generate frontend API types from Rust

**Status:** Accepted

**Decision:** DTOs derive `ts_rs::TS`. The generated files are committed to `apps/web/src/types/generated`, and a Rust test fails when they differ from what the DTOs produce. `make types` regenerates them. This is simpler than an OpenAPI toolchain for one Rust server and one client.

**Consequence:** changing a DTO without updating the frontend fails `cargo test`. Response bodies are not validated at runtime in the browser.
