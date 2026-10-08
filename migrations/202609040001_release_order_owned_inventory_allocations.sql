-- Order lifecycle is now completely inventory-neutral: `create_order` no
-- longer allocates, `cancel_order` no longer releases, and `complete_order`
-- no longer consumes requirements or posts a finished-product
-- ProductionOutput. Explicit Ticket recording (`record-acquisition` /
-- `record-production`) is the only execution/accounting mechanism; the root
-- Manufacturing ticket produces the finished item.
--
-- Any `inventory_allocations` row still keyed to an `order_requirement_id`
-- was written under the old semantics. With `cancel_order` / `complete_order`
-- no longer touching allocations, an *active* one (never released, never
-- consumed) would stay claimed forever -- no code path is left that frees it
-- -- permanently understating `available = balance - active allocations` and
-- keeping the Inventory "Reserved" figure / Reservations tab non-zero.
--
-- Free those active claims by marking them released: the existing "claim
-- abandoned before use, nothing was ever consumed" lifecycle state (see
-- `order::InventoryAllocation`). This is the exact, non-destructive pattern
-- 202609030001 already ran for the ticket-owned side. It posts NO
-- `inventory_events`, touches NO `inventory_balances`, and changes NO cost
-- basis -- it only stops the row counting against availability.
--
-- Rows already `consumed_at` (a real Consumption event was posted, e.g. by a
-- pre-slice `complete_order`) are left exactly as they are -- accounting
-- history stays intact. The table and its columns are kept; a later schema
-- cleanup can drop them once nothing reads the ledger.
UPDATE inventory_allocations
SET released_at = now()
WHERE order_requirement_id IS NOT NULL
  AND released_at IS NULL
  AND consumed_at IS NULL;
