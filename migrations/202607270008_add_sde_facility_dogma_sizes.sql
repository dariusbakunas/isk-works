ALTER TABLE sde_structure_manufacturing_modifiers
  ADD COLUMN IF NOT EXISTS structure_size bigint NULL;

ALTER TABLE sde_structure_rig_modifiers
  ADD COLUMN IF NOT EXISTS rig_size bigint NULL;
