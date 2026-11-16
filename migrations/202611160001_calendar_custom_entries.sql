-- Calendar entries the player adds by hand. `occurs_at` is the instant;
-- `time_zone` is the IANA zone it was entered in, kept so an edit shows the
-- original wall-clock time. A disconnected character leaves the entry
-- workspace-wide instead of deleting it.

CREATE TABLE calendar_custom_entries (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    connection_id uuid NULL REFERENCES eve_connections (id) ON DELETE SET NULL,
    occurs_at timestamptz NOT NULL,
    time_zone text NOT NULL CHECK (length(time_zone) BETWEEN 1 AND 64),
    title text NOT NULL CHECK (length(title) BETWEEN 1 AND 120),
    description text NOT NULL DEFAULT '' CHECK (length(description) <= 4000),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX calendar_custom_entries_workspace_occurs_at
    ON calendar_custom_entries (workspace_id, occurs_at);
