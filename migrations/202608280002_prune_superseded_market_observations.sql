-- Market order observations accumulate a batch per (source, type) on every
-- ESI refresh; only the batch that is a coverage row's current
-- `last_completed_batch_id` (or is FK-referenced by an import file / price
-- snapshot) is ever read. Superseded batches are pure dead weight -- 18M
-- orphan rows / 15 GB on the dev DB before this change.
--
-- Narrow the immutability triggers on the observation tables from
-- "BEFORE UPDATE OR DELETE" to "BEFORE UPDATE" so a superseded, unreferenced
-- batch can be garbage-collected (by `complete_esi_market_refresh`). The
-- evidence is still immutable -- it cannot be *altered* -- and the
-- `ON DELETE RESTRICT` FKs from `market_source_coverage`,
-- `market_import_file_observations` and `market_price_snapshot_observations`
-- remain the hard guarantee that anything in use cannot be deleted. The
-- `market_price_snapshot*` audit-trail triggers are left fully immutable.

DROP TRIGGER market_order_observations_immutable ON market_order_observations;
CREATE TRIGGER market_order_observations_immutable
BEFORE UPDATE ON market_order_observations
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();

DROP TRIGGER market_observation_batches_immutable ON market_observation_batches;
CREATE TRIGGER market_observation_batches_immutable
BEFORE UPDATE ON market_observation_batches
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();
