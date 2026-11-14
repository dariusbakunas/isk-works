-- PI Planner plans saved as frozen snapshots: the solve request and response
-- as the player saw them, plus which colonies they have set up in game.

CREATE TABLE pi_saved_plans (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    -- Shape of `scenario` and `solution`; the web renders by version.
    snapshot_version integer NOT NULL,
    -- The solve request, with the scenario characters' names.
    scenario jsonb NOT NULL,
    -- The solve response as shown: plan, what-ifs, templates.
    solution jsonb NOT NULL,
    -- Colony indexes (into solution.plan.colonies) ticked as set up.
    set_up_colonies integer[] NOT NULL DEFAULT '{}',
    -- Summary for the list, so it never parses the snapshots.
    isk_per_month numeric NOT NULL,
    colonies integer NOT NULL CHECK (colonies > 0),
    home_system_id bigint NOT NULL,
    home_system_name text NULL,
    price_observed_at timestamptz NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX pi_saved_plans_workspace ON pi_saved_plans (workspace_id, updated_at DESC);
