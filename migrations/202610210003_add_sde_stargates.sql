-- Stargates from the SDE's mapStargates: one row per gate, so a connection
-- appears once in each direction. Market Orders walks this graph to decide
-- whether a buy order's jump range reaches a location.
CREATE TABLE sde_stargates (
  import_id uuid NOT NULL,
  stargate_id bigint NOT NULL,
  solar_system_id bigint NOT NULL,
  destination_solar_system_id bigint NOT NULL,
  PRIMARY KEY (import_id, stargate_id),
  FOREIGN KEY (import_id, solar_system_id)
    REFERENCES sde_solar_systems(import_id, solar_system_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, destination_solar_system_id)
    REFERENCES sde_solar_systems(import_id, solar_system_id)
    ON DELETE CASCADE
);

CREATE INDEX sde_stargates_system_idx ON sde_stargates (import_id, solar_system_id);
