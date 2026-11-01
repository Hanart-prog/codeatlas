-- A backtest is deterministic for its parameters, so one row answers every repeat.
CREATE TABLE backtest_runs (
    id              INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs (id) ON DELETE CASCADE,
    params_json     TEXT    NOT NULL,
    metrics_json    TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,
    UNIQUE (analysis_run_id, params_json)
) STRICT;
