ALTER TABLE price_snapshot_items
  ADD COLUMN pricing_policy text CHECK (
    pricing_policy IS NULL OR pricing_policy IN (
      'lowest_sell',
      'highest_buy',
      'acquire_quantity_from_sell_orders',
      'liquidate_quantity_into_buy_orders'
    )
  );
