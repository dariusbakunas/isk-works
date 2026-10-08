CREATE TABLE build_draft_planning (
    build_id uuid PRIMARY KEY REFERENCES builds(id) ON DELETE CASCADE,
    planning_input jsonb NOT NULL,
    updated_at timestamptz NOT NULL
);
