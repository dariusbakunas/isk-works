CREATE TABLE sde_structure_manufacturing_modifiers (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  type_id bigint NOT NULL,
  material_reduction_percent numeric NOT NULL,
  time_reduction_percent numeric NOT NULL,
  job_cost_reduction_percent numeric NOT NULL,
  PRIMARY KEY (import_id, type_id)
);

CREATE TABLE sde_structure_rig_modifiers (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  type_id bigint NOT NULL,
  material_reduction_percent numeric NOT NULL,
  time_reduction_percent numeric NOT NULL,
  high_sec_multiplier numeric NOT NULL,
  low_sec_multiplier numeric NOT NULL,
  null_sec_multiplier numeric NOT NULL,
  compatible_structure_group_ids bigint[] NOT NULL DEFAULT '{}',
  PRIMARY KEY (import_id, type_id)
);
