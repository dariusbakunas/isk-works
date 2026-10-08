CREATE TABLE sde_solar_systems (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  solar_system_id bigint NOT NULL CHECK (solar_system_id > 0),
  name_en text NOT NULL CHECK (length(btrim(name_en)) > 0),
  PRIMARY KEY (import_id, solar_system_id)
);

CREATE INDEX sde_solar_systems_name_idx
  ON sde_solar_systems (import_id, lower(name_en));
