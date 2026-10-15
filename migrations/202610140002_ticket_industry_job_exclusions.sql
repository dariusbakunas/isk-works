-- ESI industry jobs a user unlinked from a ticket by hand. Automatic
-- matching (`plan_ticket_job_sync`) never links that job to that ticket
-- again; it may still give the job to another ticket. Linking the pair by
-- hand deletes the row.
CREATE TABLE ticket_industry_job_exclusions (
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    ticket_id uuid NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    job_id bigint NOT NULL,
    excluded_at timestamptz NOT NULL,
    PRIMARY KEY (workspace_id, ticket_id, job_id)
);

CREATE INDEX ticket_industry_job_exclusions_ticket_idx
    ON ticket_industry_job_exclusions (ticket_id);
