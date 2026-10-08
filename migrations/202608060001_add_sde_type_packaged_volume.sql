ALTER TABLE sde_types
  ADD COLUMN packaged_volume_m3 numeric NULL;

ALTER TABLE sde_types
  ADD CONSTRAINT sde_types_packaged_volume_non_negative
  CHECK (packaged_volume_m3 IS NULL OR packaged_volume_m3 >= 0);
