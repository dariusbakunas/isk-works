CREATE TABLE inventory_balances (
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  quantity bigint NOT NULL CHECK (quantity >= 0),
  known_quantity bigint NOT NULL CHECK (known_quantity >= 0),
  unknown_quantity bigint NOT NULL CHECK (unknown_quantity >= 0),
  total_historical_cost numeric(24, 4) NOT NULL CHECK (total_historical_cost >= 0),
  revision bigint NOT NULL CHECK (revision > 0),
  last_activity_at timestamptz NOT NULL,
  PRIMARY KEY (workspace_id, owner_id, type_id),
  CONSTRAINT inventory_balances_name_not_empty CHECK (length(btrim(captured_name)) > 0),
  CONSTRAINT inventory_balances_quantities_consistent
    CHECK (quantity = known_quantity + unknown_quantity),
  CONSTRAINT inventory_balances_empty_value_consistent
    CHECK (quantity > 0 OR total_historical_cost = 0)
);

CREATE INDEX inventory_balances_workspace_owner_activity_idx
  ON inventory_balances (workspace_id, owner_id, last_activity_at DESC);

CREATE TABLE inventory_events (
  id uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  owner_id uuid NOT NULL REFERENCES owners(id) ON DELETE RESTRICT,
  type_id bigint NOT NULL CHECK (type_id > 0),
  captured_name text NOT NULL,
  event_kind text NOT NULL,
  quantity_delta bigint NOT NULL CHECK (quantity_delta <> 0),
  known_quantity_delta bigint NOT NULL,
  unknown_quantity_delta bigint NOT NULL,
  total_cost_delta numeric(24, 4) NOT NULL,
  unit_cost numeric(24, 4) CHECK (unit_cost >= 0),
  cost_quality text NOT NULL,
  source_reference text NOT NULL DEFAULT '',
  note text NOT NULL DEFAULT '',
  effective_at timestamptz NOT NULL,
  recorded_at timestamptz NOT NULL,
  sequence bigint NOT NULL CHECK (sequence > 0),
  reverses_event_id uuid REFERENCES inventory_events(id) ON DELETE RESTRICT,
  resulting_quantity bigint NOT NULL CHECK (resulting_quantity >= 0),
  resulting_known_quantity bigint NOT NULL CHECK (resulting_known_quantity >= 0),
  resulting_unknown_quantity bigint NOT NULL CHECK (resulting_unknown_quantity >= 0),
  resulting_total_cost numeric(24, 4) NOT NULL CHECK (resulting_total_cost >= 0),
  resulting_average_cost numeric(24, 4) CHECK (resulting_average_cost >= 0),
  resulting_revision bigint NOT NULL CHECK (resulting_revision > 0),
  CONSTRAINT inventory_events_name_not_empty CHECK (length(btrim(captured_name)) > 0),
  CONSTRAINT inventory_events_kind_valid
    CHECK (event_kind IN ('opening_balance', 'purchase', 'reversal')),
  CONSTRAINT inventory_events_quality_valid
    CHECK (cost_quality IN ('known', 'estimated', 'unknown', 'zero_cost')),
  CONSTRAINT inventory_events_delta_consistent
    CHECK (quantity_delta = known_quantity_delta + unknown_quantity_delta),
  CONSTRAINT inventory_events_result_quantities_consistent
    CHECK (resulting_quantity = resulting_known_quantity + resulting_unknown_quantity),
  CONSTRAINT inventory_events_average_consistent
    CHECK (
      (resulting_quantity = 0 AND resulting_average_cost IS NULL)
      OR (resulting_unknown_quantity > 0 AND resulting_average_cost IS NULL)
      OR (resulting_quantity > 0 AND resulting_unknown_quantity = 0 AND resulting_average_cost IS NOT NULL)
    ),
  CONSTRAINT inventory_events_reversal_link_consistent
    CHECK (
      (event_kind = 'reversal' AND reverses_event_id IS NOT NULL)
      OR (event_kind <> 'reversal' AND reverses_event_id IS NULL)
    ),
  UNIQUE (workspace_id, owner_id, type_id, sequence),
  UNIQUE (reverses_event_id)
);

CREATE INDEX inventory_events_item_sequence_idx
  ON inventory_events (workspace_id, owner_id, type_id, sequence);
