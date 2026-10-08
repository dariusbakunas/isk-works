CREATE TABLE workspaces (
  id uuid PRIMARY KEY,
  display_name text NOT NULL,
  owner_id uuid NOT NULL UNIQUE,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  CONSTRAINT workspaces_display_name_not_empty CHECK (length(trim(display_name)) > 0)
);

CREATE UNIQUE INDEX workspaces_singleton_idx ON workspaces ((true));

CREATE TABLE owners (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  owner_kind text NOT NULL,
  display_name text NOT NULL,
  eve_owner_id bigint NULL,
  hidden boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  CONSTRAINT owners_kind_supported CHECK (owner_kind IN ('manual')),
  CONSTRAINT owners_display_name_not_empty CHECK (length(trim(display_name)) > 0),
  CONSTRAINT owners_manual_eve_owner_absent CHECK (
    owner_kind != 'manual' OR eve_owner_id IS NULL
  )
);

ALTER TABLE workspaces
  ADD CONSTRAINT workspaces_owner_id_fk
  FOREIGN KEY (owner_id)
  REFERENCES owners(id)
  DEFERRABLE INITIALLY DEFERRED;

CREATE UNIQUE INDEX owners_one_hidden_manual_per_workspace_idx
  ON owners (workspace_id)
  WHERE owner_kind = 'manual' AND hidden = true;
