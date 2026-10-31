-- PI Planner inputs the player enters per planet, reused by every scenario.
-- Planet and type ids are SDE ids; no foreign key, since SDE re-imports
-- replace those tables.

CREATE TABLE pi_planet_densities (
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    planet_id bigint NOT NULL,
    resource_type_id bigint NOT NULL,
    -- The scan tooltip's "Resource Density" %.
    density_pct numeric NULL CHECK (density_pct >= 0 AND density_pct <= 100),
    -- The average head value from a draft extractor program.
    head_value numeric NULL CHECK (head_value > 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, planet_id, resource_type_id),
    CHECK (density_pct IS NOT NULL OR head_value IS NOT NULL)
);

CREATE TABLE pi_planet_settings (
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    planet_id bigint NOT NULL,
    -- Overrides the scenario's default customs rate (0.1 = 10 %).
    customs_rate numeric NULL CHECK (customs_rate >= 0 AND customs_rate <= 1),
    -- Other players extract here.
    contested boolean NOT NULL DEFAULT false,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, planet_id),
    CHECK (customs_rate IS NOT NULL OR contested)
);
