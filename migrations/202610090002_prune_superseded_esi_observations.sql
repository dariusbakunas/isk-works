-- Adjusted prices accumulate a full copy of every type (~16k rows) on each
-- refresh, but only each type's latest observation is ever read
-- (`latest_adjusted_prices`). Superseded rows are dead weight -- 4M rows /
-- 1.6 GB in prod before this change (issue #22).
--
-- Narrow the immutability trigger from "BEFORE UPDATE OR DELETE" to
-- "BEFORE UPDATE", as 202608280002 did for market order observations, so the
-- worker can garbage-collect superseded rows. An observation still cannot be
-- altered. Asset snapshots carry no such trigger; their pruning needs no
-- schema change.

DROP TRIGGER industry_adjusted_price_observations_immutable
  ON industry_adjusted_price_observations;
CREATE TRIGGER industry_adjusted_price_observations_immutable
BEFORE UPDATE ON industry_adjusted_price_observations
FOR EACH ROW EXECUTE FUNCTION reject_market_evidence_mutation();
