-- EVE's industry rig applicability data (industryModifierSources +
-- industryTargetFilters, present in CCP JSONL SDE build >= ~3475087).
--
-- A structure/reaction rig's material and time bonuses each apply only to
-- jobs whose produced item falls in a named target filter (a category/group
-- set). Older SDE imports don't carry this; their rigs keep NULL filter ids
-- and are treated as unrestricted, i.e. unchanged pre-feature behavior.

CREATE TABLE sde_industry_target_filters (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  filter_id bigint NOT NULL,
  name_en text NOT NULL,
  category_ids bigint[] NOT NULL DEFAULT '{}',
  group_ids bigint[] NOT NULL DEFAULT '{}',
  PRIMARY KEY (import_id, filter_id)
);

ALTER TABLE sde_structure_rig_modifiers
  ADD COLUMN material_filter_id bigint NULL,
  ADD COLUMN time_filter_id bigint NULL;

ALTER TABLE sde_reaction_rig_modifiers
  ADD COLUMN material_filter_id bigint NULL,
  ADD COLUMN time_filter_id bigint NULL;
