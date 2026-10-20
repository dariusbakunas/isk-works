-- Every ESI industry job ISK Works has seen for a connected character:
-- running ones and, since the sync asks for `include_completed=true`, the
-- last 90 days of finished ones (delivered / cancelled / reverted). Rows are
-- upserted on every jobs sync and kept after ESI drops them, so the Jobs
-- history outlives ESI's 90-day window. Removed with the connection.
CREATE TABLE character_industry_jobs (
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE CASCADE,
    job_id bigint NOT NULL,
    activity_id bigint NOT NULL,
    blueprint_id bigint,
    blueprint_type_id bigint NOT NULL,
    product_type_id bigint,
    facility_id bigint NOT NULL,
    station_id bigint,
    runs bigint NOT NULL,
    licensed_runs bigint,
    successful_runs bigint,
    probability numeric,
    cost numeric(24,4),
    status text NOT NULL,
    start_date timestamptz NOT NULL,
    end_date timestamptz NOT NULL,
    pause_date timestamptz,
    completed_date timestamptz,
    completed_character_id bigint,
    first_seen_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    PRIMARY KEY (workspace_id, job_id)
);

CREATE INDEX character_industry_jobs_end_idx
    ON character_industry_jobs (workspace_id, end_date DESC);
CREATE INDEX character_industry_jobs_completed_idx
    ON character_industry_jobs (workspace_id, completed_date DESC)
    WHERE completed_date IS NOT NULL;
CREATE INDEX character_industry_jobs_connection_idx
    ON character_industry_jobs (connection_id);
