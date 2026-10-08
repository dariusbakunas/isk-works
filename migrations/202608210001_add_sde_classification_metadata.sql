ALTER TABLE sde_imports
  ADD COLUMN category_count bigint NOT NULL DEFAULT 0,
  ADD COLUMN group_count bigint NOT NULL DEFAULT 0,
  ADD COLUMN meta_group_count bigint NOT NULL DEFAULT 0,
  ADD COLUMN market_group_count bigint NOT NULL DEFAULT 0,
  ADD COLUMN classified_type_count bigint NOT NULL DEFAULT 0;

CREATE TABLE sde_categories (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  category_id bigint NOT NULL,
  name_en text NOT NULL,
  published boolean NOT NULL,
  PRIMARY KEY (import_id, category_id)
);
CREATE TABLE sde_groups (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  group_id bigint NOT NULL,
  name_en text NOT NULL,
  category_id bigint,
  published boolean NOT NULL,
  PRIMARY KEY (import_id, group_id),
  FOREIGN KEY (import_id, category_id) REFERENCES sde_categories(import_id, category_id)
);
CREATE TABLE sde_meta_groups (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  meta_group_id bigint NOT NULL,
  name_en text NOT NULL,
  PRIMARY KEY (import_id, meta_group_id)
);
CREATE TABLE sde_market_groups (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  market_group_id bigint NOT NULL,
  name_en text NOT NULL,
  parent_group_id bigint,
  PRIMARY KEY (import_id, market_group_id),
  FOREIGN KEY (import_id, parent_group_id)
    REFERENCES sde_market_groups(import_id, market_group_id) DEFERRABLE INITIALLY DEFERRED
);

ALTER TABLE sde_types ADD COLUMN meta_group_id bigint;

CREATE INDEX sde_groups_category_idx ON sde_groups(import_id, category_id);
CREATE INDEX sde_types_meta_group_idx ON sde_types(import_id, meta_group_id);
CREATE INDEX sde_types_market_group_idx ON sde_types(import_id, market_group_id);
CREATE INDEX sde_market_groups_parent_idx ON sde_market_groups(import_id, parent_group_id);
