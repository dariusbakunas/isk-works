-- Workspace default facility per EVE industry target filter (issue #12).
-- target_filter_id is a CCP industryTargetFilters id (stable across SDE
-- imports); NULL is the role's fallback for products no defaulted target
-- matches. New root and producer Builds take their facility from here.
CREATE TABLE industry_facility_defaults (
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    role text NOT NULL CHECK (role IN ('manufacturing', 'reaction')),
    target_filter_id bigint,
    facility_profile_id uuid NOT NULL REFERENCES industry_facility_profiles(id) ON DELETE CASCADE,
    rig_covered_at_assignment boolean NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX industry_facility_defaults_target
    ON industry_facility_defaults (workspace_id, role, coalesce(target_filter_id, 0));

CREATE INDEX industry_facility_defaults_facility
    ON industry_facility_defaults (facility_profile_id);
