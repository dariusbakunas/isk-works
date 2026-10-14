-- ESI industry jobs linked to the Board ticket they were run for, with the
-- job's last known state. A link is made while the ticket is In Progress
-- (same assignee, activity and product, started after the ticket was
-- created) and kept afterwards, so it survives the ticket moving to
-- Complete and the job leaving ESI on delivery (status 'delivered'). A job
-- belongs to one ticket at most.
--
-- `cost` is what installing the job cost (ESI's installation fee plus
-- facility tax); recording the ticket's outcome prefills from it.
CREATE TABLE ticket_industry_jobs (
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    job_id bigint NOT NULL,
    ticket_id uuid NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE CASCADE,
    activity_id bigint NOT NULL,
    blueprint_type_id bigint NOT NULL,
    product_type_id bigint,
    facility_id bigint NOT NULL,
    runs bigint NOT NULL CHECK (runs > 0),
    cost numeric(24,4) CHECK (cost >= 0),
    status text NOT NULL,
    start_date timestamptz NOT NULL,
    end_date timestamptz NOT NULL,
    pause_date timestamptz,
    linked_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    PRIMARY KEY (workspace_id, job_id)
);

CREATE INDEX ticket_industry_jobs_ticket_idx ON ticket_industry_jobs (ticket_id);
