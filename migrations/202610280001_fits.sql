-- Saved ship fits. A fit belongs to the workspace (like skill plans) and is
-- calculated for any of its characters. Type ids have no FK to sde_types:
-- SDE rows are per import, and a fit must survive re-imports.
CREATE TABLE fits (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name text NOT NULL CHECK (length(trim(name)) > 0),
  ship_type_id bigint NOT NULL,
  description text NOT NULL DEFAULT '',
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL
);

CREATE INDEX fits_workspace_ship_idx ON fits (workspace_id, ship_type_id);

-- What a fit holds, in the order it was saved. Writes replace a fit's
-- items whole. slot_index is the position in a rack; bays and cargo have
-- none. state is the requested state; the calculation reports the reached
-- one.
CREATE TABLE fit_items (
  fit_id uuid NOT NULL REFERENCES fits(id) ON DELETE CASCADE,
  position integer NOT NULL CHECK (position >= 0),
  slot_kind text NOT NULL,
  slot_index smallint NULL,
  type_id bigint NOT NULL,
  quantity integer NOT NULL CHECK (quantity >= 1),
  state text NOT NULL,
  charge_type_id bigint NULL,
  -- {baseTypeId, attributes {attributeId: value}} for a mutated module.
  mutation jsonb NULL,
  PRIMARY KEY (fit_id, position),
  CONSTRAINT fit_items_slot_kind_known CHECK (slot_kind IN (
    'high', 'med', 'low', 'rig', 'subsystem', 'service', 'fighter_tube',
    'fighter_bay', 'drone_bay', 'cargo', 'implant', 'booster'
  )),
  CONSTRAINT fit_items_state_known CHECK (state IN ('offline', 'online', 'active', 'overload'))
);

CREATE UNIQUE INDEX fit_items_slot_unique
  ON fit_items (fit_id, slot_kind, slot_index)
  WHERE slot_index IS NOT NULL;
