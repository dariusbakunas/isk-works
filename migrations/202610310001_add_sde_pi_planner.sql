-- PI Planner static data: planet types and radii, which processors run each
-- schematic, PI structure CPU/power figures, and commodity customs tax
-- multipliers. Older imports leave the new planet columns NULL and are redone
-- by the importer's pi_planner_data requirement.
ALTER TABLE sde_planets
  ADD COLUMN type_id bigint NULL,
  ADD COLUMN radius_m bigint NULL;

CREATE TABLE sde_planet_schematic_pins (
  import_id uuid NOT NULL,
  schematic_id bigint NOT NULL,
  pin_type_id bigint NOT NULL,
  PRIMARY KEY (import_id, schematic_id, pin_type_id),
  FOREIGN KEY (import_id, schematic_id)
    REFERENCES sde_planet_schematics(import_id, schematic_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, pin_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE
);

CREATE TABLE sde_planet_pin_types (
  import_id uuid NOT NULL,
  type_id bigint NOT NULL,
  kind text NOT NULL,
  planet_type_id bigint NULL,
  harvester_type_id bigint NULL,
  cpu_load numeric NULL,
  power_load numeric NULL,
  cpu_output numeric NULL,
  power_output numeric NULL,
  ecu_head_cpu numeric NULL,
  ecu_head_power numeric NULL,
  cpu_load_per_km numeric NULL,
  power_load_per_km numeric NULL,
  cpu_load_level_modifier numeric NULL,
  power_load_level_modifier numeric NULL,
  PRIMARY KEY (import_id, type_id),
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  CONSTRAINT sde_planet_pin_types_kind_check CHECK (kind IN (
    'extractor_resource', 'command_center', 'processor', 'storage',
    'launchpad', 'link', 'ecu'
  ))
);

CREATE TABLE sde_planet_commodities (
  import_id uuid NOT NULL,
  type_id bigint NOT NULL,
  export_tax_multiplier numeric NOT NULL,
  import_tax_multiplier numeric NOT NULL,
  PRIMARY KEY (import_id, type_id),
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  CONSTRAINT sde_planet_commodities_multipliers_non_negative
    CHECK (export_tax_multiplier >= 0 AND import_tax_multiplier >= 0)
);
