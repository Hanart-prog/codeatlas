# CodeAtlas

Analyze a local Git repository's history and source dependencies to explain architecture evolution, estimate change impact, and backtest those estimates against later commits.

> **Status: Phase 3 (change impact).** Given changed files and a historical cutoff, CodeAtlas ranks the files to inspect, with import and co-change evidence kept separate. It also indexes first-parent history, reports churn, and shows the import graph at any commit. **Architecture comparison and backtesting do not exist yet:** `compare`, `backtest`, `report` and `demo` exit with an error naming the phase that delivers them.

Rankings are a heuristic. A score is not a probability of failure, and co-change is correlation, not causation.

CodeAtlas is local-first: no accounts, cloud services, paid APIs, LLMs or telemetry. It treats analyzed repositories as untrusted, read-only input and never executes their code, hooks or scripts.

## Requirements

- Rust stable, from [rustup](https://rustup.rs). `rust-toolchain.toml` selects the components.
- Node.js 20.16 or newer.
- pnpm 10. Optional: without it, the Makefile runs the pinned version through `npx`.
- A C compiler for the bundled SQLite.

## Getting started

```bash
make setup   # fetch Rust and frontend dependencies
make dev     # API on 127.0.0.1:4317, dashboard with live reload on http://127.0.0.1:5173
```

| Command | Purpose |
|---|---|
| `make check` | rustfmt, clippy with warnings denied, ESLint, TypeScript |
| `make test` | All Rust and frontend tests |
| `make build` | Release binary and production frontend |
| `make e2e` | Browser tests; `PW_CHANNEL=chrome make e2e` uses local Chrome |
| `make types` | Regenerate frontend types from the Rust DTOs |
| `make fixture DIR=/empty/dir` | Generate the deterministic fixture repository |

Open the dashboard and enter a repository path, or use the CLI. After `make build`, `codeatlas serve` run from the repository root also serves the dashboard at http://127.0.0.1:4317.

## CLI

```bash
cargo run -p atlas-cli -- analyze /path/to/repo --ref HEAD --history 500
cargo run -p atlas-cli -- overview /path/to/repo --format json
cargo run -p atlas-cli -- graph /path/to/repo --ref <commit> --directory src --format json
cargo run -p atlas-cli -- impact /path/to/repo --ref <commit> --file src/auth/session.ts --top 10
cargo run -p atlas-cli -- serve /path/to/repo --port 4317
curl http://127.0.0.1:4317/api/v1/repositories
```

Results go to stdout and logs to stderr. Rerunning `analyze` on the same commit and settings reuses the stored result. `--data-dir` or `CODEATLAS_DATA_DIR` chooses where the database lives, and `--config` loads a TOML file of the settings in [config.rs](crates/atlas-model/src/config.rs).

## Documentation

- [Architecture](docs/architecture.md)
- [Data model](docs/data-model.md)
- [Algorithms and metric definitions](docs/algorithms.md)
- [Limitations](docs/limitations.md)
- [Specification review](docs/spec-review.md)
- [Decision records](docs/decisions/)

## License

[MIT](LICENSE)
