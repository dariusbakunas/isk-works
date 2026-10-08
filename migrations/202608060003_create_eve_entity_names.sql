CREATE TABLE eve_entity_names (
  entity_id bigint PRIMARY KEY CHECK (entity_id > 0),
  entity_name text NOT NULL CHECK (length(btrim(entity_name)) > 0),
  category text NOT NULL CHECK (length(btrim(category)) > 0),
  observed_at timestamptz NOT NULL
);

CREATE INDEX eve_entity_names_name_idx ON eve_entity_names (lower(entity_name));
