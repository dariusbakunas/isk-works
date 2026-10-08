ALTER TABLE sde_imports
  DROP CONSTRAINT sde_imports_counts_non_negative;

ALTER TABLE sde_imports
  ADD COLUMN skipped_blueprint_count bigint NOT NULL DEFAULT 0;

ALTER TABLE sde_imports
  ADD CONSTRAINT sde_imports_counts_non_negative CHECK (
    type_count >= 0
    AND blueprint_count >= 0
    AND material_line_count >= 0
    AND product_line_count >= 0
    AND skipped_blueprint_count >= 0
  );
