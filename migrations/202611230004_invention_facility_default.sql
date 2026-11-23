-- The workspace's default invention facility. Science rigs carry no target
-- filter, so one facility serves every invention job: a single row per
-- workspace instead of a row per rig target. Either facility role is
-- allowed; the Invention page pre-selects it.
CREATE TABLE industry_invention_facility_defaults (
    workspace_id uuid PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    facility_profile_id uuid NOT NULL REFERENCES industry_facility_profiles(id) ON DELETE CASCADE,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX industry_invention_facility_defaults_facility
    ON industry_invention_facility_defaults (facility_profile_id);
