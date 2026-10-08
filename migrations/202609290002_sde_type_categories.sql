-- Analytics category per SDE type, rolled up from the market-group tree by
-- `iskworks_core::classify_all` and rebuilt whenever an SDE import is written
-- (or on startup for an active import that predates this table). Read by
-- Finance analytics and the Transactions `category` filter so neither walks
-- the market-group tree per query.
CREATE TABLE sde_type_categories (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  type_id bigint NOT NULL,
  category text NOT NULL,
  PRIMARY KEY (import_id, type_id)
);
CREATE INDEX sde_type_categories_category_idx ON sde_type_categories (import_id, category);
