-- Drop the wallet-purchase import review.
--
-- Each synced Market Buy used to get a row in inventory_import_proposals,
-- reviewed (accept / ignore / reopen) on the Inventory "Wallet Imports" page.
-- That page is gone: Finance records purchases straight from
-- esi_wallet_transactions, with inventory_event_sources as the only recording
-- state. Ignored purchases simply become recordable again.

DROP TABLE inventory_import_proposals;

ALTER TABLE esi_sync_runs DROP COLUMN proposal_count;
