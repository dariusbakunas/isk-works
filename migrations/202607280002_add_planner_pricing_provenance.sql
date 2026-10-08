ALTER TABLE price_snapshot_items
  ADD COLUMN item_role text NOT NULL DEFAULT 'material'
    CHECK (item_role IN ('material', 'output')),
  ADD COLUMN selection_kind text NOT NULL DEFAULT 'default'
    CHECK (selection_kind IN ('default', 'market_policy', 'manual')),
  ADD COLUMN manual_unit_price numeric(24, 4)
    CHECK (manual_unit_price >= 0),
  ADD CONSTRAINT price_snapshot_items_manual_consistent CHECK (
    (selection_kind = 'manual' AND manual_unit_price IS NOT NULL)
    OR (selection_kind <> 'manual' AND manual_unit_price IS NULL)
  );

ALTER TABLE price_snapshot_items
  DROP CONSTRAINT price_snapshot_items_pkey,
  ADD PRIMARY KEY (price_snapshot_id, type_id, item_role);
