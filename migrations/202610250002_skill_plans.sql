-- Skill plans: ordered (skill, level) entries a workspace's characters train
-- through, the way the client lists a plan (each level its own entry,
-- prerequisites first). Plans belong to the workspace, not a character.
CREATE TABLE skill_plans (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name text NOT NULL CHECK (length(trim(name)) > 0),
  description text NOT NULL DEFAULT '',
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL
);

CREATE INDEX skill_plans_workspace_idx ON skill_plans (workspace_id);

CREATE TABLE skill_plan_entries (
  plan_id uuid NOT NULL REFERENCES skill_plans(id) ON DELETE CASCADE,
  position integer NOT NULL CHECK (position >= 0),
  skill_type_id bigint NOT NULL,
  level smallint NOT NULL CHECK (level BETWEEN 1 AND 5),
  PRIMARY KEY (plan_id, position),
  UNIQUE (plan_id, skill_type_id, level)
);
