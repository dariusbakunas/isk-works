CREATE TABLE sde_imports (
  id uuid PRIMARY KEY,
  source_version text NOT NULL,
  source_label text NOT NULL,
  source_checksum text NOT NULL,
  status text NOT NULL,
  active boolean NOT NULL DEFAULT false,
  started_at timestamptz NOT NULL,
  completed_at timestamptz NULL,
  type_count bigint NOT NULL DEFAULT 0,
  blueprint_count bigint NOT NULL DEFAULT 0,
  material_line_count bigint NOT NULL DEFAULT 0,
  product_line_count bigint NOT NULL DEFAULT 0,
  error_message text NULL,
  CONSTRAINT sde_imports_status_supported
    CHECK (status IN ('importing', 'active', 'superseded', 'failed')),
  CONSTRAINT sde_imports_checksum_not_empty CHECK (length(trim(source_checksum)) > 0),
  CONSTRAINT sde_imports_label_not_empty CHECK (length(trim(source_label)) > 0),
  CONSTRAINT sde_imports_version_not_empty CHECK (length(trim(source_version)) > 0),
  CONSTRAINT sde_imports_counts_non_negative CHECK (
    type_count >= 0
    AND blueprint_count >= 0
    AND material_line_count >= 0
    AND product_line_count >= 0
  )
);

CREATE UNIQUE INDEX sde_imports_one_active_idx
  ON sde_imports ((true))
  WHERE active = true;

CREATE INDEX sde_imports_checksum_idx ON sde_imports (source_checksum);

CREATE TABLE sde_types (
  import_id uuid NOT NULL REFERENCES sde_imports(id) ON DELETE CASCADE,
  type_id bigint NOT NULL,
  name_en text NOT NULL,
  group_id bigint NULL,
  group_name_en text NULL,
  market_group_id bigint NULL,
  published boolean NOT NULL,
  PRIMARY KEY (import_id, type_id),
  CONSTRAINT sde_types_type_id_positive CHECK (type_id > 0),
  CONSTRAINT sde_types_name_not_empty CHECK (length(trim(name_en)) > 0)
);

CREATE INDEX sde_types_name_idx
  ON sde_types (import_id, lower(name_en) text_pattern_ops);

CREATE TABLE sde_blueprints (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  name_en text NOT NULL,
  duration_seconds bigint NULL,
  PRIMARY KEY (import_id, blueprint_type_id),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  CONSTRAINT sde_blueprints_duration_positive
    CHECK (duration_seconds IS NULL OR duration_seconds > 0)
);

CREATE INDEX sde_blueprints_name_idx
  ON sde_blueprints (import_id, lower(name_en) text_pattern_ops);

CREATE TABLE sde_blueprint_materials (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  material_type_id bigint NOT NULL,
  quantity bigint NOT NULL,
  position integer NOT NULL,
  PRIMARY KEY (import_id, blueprint_type_id, material_type_id),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_blueprints(import_id, blueprint_type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, material_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT,
  CONSTRAINT sde_blueprint_materials_quantity_positive CHECK (quantity > 0),
  CONSTRAINT sde_blueprint_materials_position_non_negative CHECK (position >= 0)
);

CREATE TABLE sde_blueprint_products (
  import_id uuid NOT NULL,
  blueprint_type_id bigint NOT NULL,
  product_type_id bigint NOT NULL,
  quantity bigint NOT NULL,
  position integer NOT NULL,
  PRIMARY KEY (import_id, blueprint_type_id, product_type_id),
  FOREIGN KEY (import_id, blueprint_type_id)
    REFERENCES sde_blueprints(import_id, blueprint_type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, product_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE RESTRICT,
  CONSTRAINT sde_blueprint_products_quantity_positive CHECK (quantity > 0),
  CONSTRAINT sde_blueprint_products_position_non_negative CHECK (position >= 0)
);

CREATE INDEX sde_blueprint_products_product_idx
  ON sde_blueprint_products (import_id, product_type_id);
