-- Every build actually pulled into a Plan's recursive graph (the root
-- plus every linked build reached), captured at the revision seen during
-- generation -- drives multi-build staleness detection so the out-of-date
-- notice can name every build that changed, not just the root.
CREATE TABLE plan_source_builds (
  plan_id uuid NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
  build_id uuid NOT NULL REFERENCES builds(id) ON DELETE RESTRICT,
  captured_revision bigint NOT NULL CHECK (captured_revision > 0),
  PRIMARY KEY (plan_id, build_id)
);

CREATE INDEX plan_source_builds_build_id_idx ON plan_source_builds (build_id);
