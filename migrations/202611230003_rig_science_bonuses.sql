-- Invention and copying bonuses of structure rigs.
--
-- A rig's dogma magnitudes are generic (one time, one material and one cost
-- attribute); `industryModifierSources` says which activity they belong
-- to. Until now every rig with a time or material attribute was imported
-- as a manufacturing rig, so a lab rig's time bonus was read as a
-- manufacturing one and its cost bonus was dropped.
--
-- The SDE tables hold the base percentages, before the security
-- multiplier; they stay 0 until the SDE is imported again. The reaction
-- table gets the columns too, always 0, so both rig tables keep one shape
-- for the queries they share.
--
-- A facility profile's rig captures the percentages for its security
-- class, as it does for its material and time reduction.

ALTER TABLE sde_structure_rig_modifiers
  ADD COLUMN invention_cost_reduction_percent numeric NOT NULL DEFAULT 0,
  ADD COLUMN invention_time_reduction_percent numeric NOT NULL DEFAULT 0,
  ADD COLUMN copying_cost_reduction_percent numeric NOT NULL DEFAULT 0,
  ADD COLUMN copying_time_reduction_percent numeric NOT NULL DEFAULT 0;

ALTER TABLE sde_reaction_rig_modifiers
  ADD COLUMN invention_cost_reduction_percent numeric NOT NULL DEFAULT 0,
  ADD COLUMN invention_time_reduction_percent numeric NOT NULL DEFAULT 0,
  ADD COLUMN copying_cost_reduction_percent numeric NOT NULL DEFAULT 0,
  ADD COLUMN copying_time_reduction_percent numeric NOT NULL DEFAULT 0;

ALTER TABLE industry_facility_profile_rigs
  ADD COLUMN invention_cost_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0
    CHECK (invention_cost_reduction_percent BETWEEN 0 AND 99.999999),
  ADD COLUMN invention_time_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0
    CHECK (invention_time_reduction_percent BETWEEN 0 AND 99.999999),
  ADD COLUMN copying_cost_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0
    CHECK (copying_cost_reduction_percent BETWEEN 0 AND 99.999999),
  ADD COLUMN copying_time_reduction_percent numeric(9, 6) NOT NULL DEFAULT 0
    CHECK (copying_time_reduction_percent BETWEEN 0 AND 99.999999);
