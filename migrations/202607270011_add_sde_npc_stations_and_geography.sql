ALTER TABLE sde_solar_systems
  ADD COLUMN constellation_id bigint,
  ADD COLUMN region_id bigint;

CREATE TABLE sde_regions (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  region_id bigint NOT NULL CHECK (region_id > 0),
  name_en text NOT NULL CHECK (length(btrim(name_en)) > 0),
  PRIMARY KEY (import_id, region_id)
);

CREATE TABLE sde_constellations (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  constellation_id bigint NOT NULL CHECK (constellation_id > 0),
  name_en text NOT NULL CHECK (length(btrim(name_en)) > 0),
  region_id bigint NOT NULL CHECK (region_id > 0),
  PRIMARY KEY (import_id, constellation_id),
  FOREIGN KEY (import_id, region_id)
    REFERENCES sde_regions(import_id, region_id)
    ON DELETE CASCADE
);

CREATE TABLE sde_npc_stations (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  station_id bigint NOT NULL CHECK (station_id > 0),
  name_en text NOT NULL CHECK (length(btrim(name_en)) > 0),
  solar_system_id bigint NOT NULL CHECK (solar_system_id > 0),
  owner_corporation_id bigint NOT NULL CHECK (owner_corporation_id > 0),
  station_type_id bigint NOT NULL CHECK (station_type_id > 0),
  PRIMARY KEY (import_id, station_id),
  FOREIGN KEY (import_id, solar_system_id)
    REFERENCES sde_solar_systems(import_id, solar_system_id)
    ON DELETE CASCADE
);

CREATE INDEX sde_regions_name_idx
  ON sde_regions (import_id, lower(name_en));
CREATE INDEX sde_constellations_name_idx
  ON sde_constellations (import_id, lower(name_en));
CREATE INDEX sde_npc_stations_name_idx
  ON sde_npc_stations (import_id, lower(name_en));
CREATE INDEX sde_npc_stations_system_idx
  ON sde_npc_stations (import_id, solar_system_id);
