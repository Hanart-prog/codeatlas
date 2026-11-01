-- The latest analysis is the one last asked for, which can be an older run served
-- from cache. Time cannot order two runs within a second.
ALTER TABLE analysis_runs ADD COLUMN used_order INTEGER NOT NULL DEFAULT 0;
