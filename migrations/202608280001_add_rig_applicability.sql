-- Persist each fitted rig's resolved applicability (which produced-item
-- category/group its material and time bonuses may affect), so the Build
-- calculation loads the same value that was resolved from the SDE at save
-- time. Rows written before this column existed get the "unresolved" default
-- and are healed on read (see facility::load_profile).

ALTER TABLE industry_facility_profile_rigs
  ADD COLUMN applicability jsonb NOT NULL
    DEFAULT '{"material":"unrestricted","time":"unrestricted"}'::jsonb;
