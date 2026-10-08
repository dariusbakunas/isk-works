-- Planetary Interaction static data: PI factory schematics and planets.
CREATE TABLE sde_planet_schematics (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  schematic_id bigint NOT NULL,
  name_en text NOT NULL,
  cycle_time_seconds bigint NOT NULL,
  PRIMARY KEY (import_id, schematic_id),
  CONSTRAINT sde_planet_schematics_cycle_time_positive CHECK (cycle_time_seconds > 0)
);

CREATE TABLE sde_planet_schematic_types (
  import_id uuid NOT NULL,
  schematic_id bigint NOT NULL,
  type_id bigint NOT NULL,
  is_input boolean NOT NULL,
  quantity bigint NOT NULL,
  PRIMARY KEY (import_id, schematic_id, type_id, is_input),
  FOREIGN KEY (import_id, schematic_id)
    REFERENCES sde_planet_schematics(import_id, schematic_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT,
  CONSTRAINT sde_planet_schematic_types_quantity_positive CHECK (quantity > 0)
);

CREATE TABLE sde_planets (
  import_id uuid NOT NULL,
  planet_id bigint NOT NULL,
  solar_system_id bigint NOT NULL,
  celestial_index bigint NOT NULL,
  name_en text NOT NULL,
  PRIMARY KEY (import_id, planet_id),
  FOREIGN KEY (import_id, solar_system_id)
    REFERENCES sde_solar_systems(import_id, solar_system_id)
    ON DELETE CASCADE
);
