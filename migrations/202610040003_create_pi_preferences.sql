CREATE TABLE pi_preferences (
  workspace_id uuid PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
  excluded_exports jsonb NOT NULL DEFAULT '[]'::jsonb,
  character_order jsonb NOT NULL DEFAULT '[]'::jsonb,
  updated_at timestamptz NOT NULL DEFAULT now()
);
