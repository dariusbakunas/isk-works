-- Price snapshots (market_price_snapshots / _lines / _observations) are an
-- immutable audit trail: no UPDATE, no DELETE. Erasing a deleted user's
-- workspace has to remove them, so allow DELETE -- and only DELETE, only on
-- those three tables -- inside a transaction that has explicitly opted in
-- with `SET LOCAL iskworks.erase_workspace = 'on'` (done solely by the
-- workspace-erase transaction in iskworks-storage). Every other mutation,
-- and every other table sharing this trigger function, stays immutable.
CREATE OR REPLACE FUNCTION reject_market_evidence_mutation() RETURNS trigger
LANGUAGE plpgsql AS $function$
BEGIN
  IF TG_OP = 'DELETE'
     AND TG_TABLE_NAME IN (
       'market_price_snapshots',
       'market_price_snapshot_lines',
       'market_price_snapshot_observations'
     )
     AND current_setting('iskworks.erase_workspace', true) = 'on' THEN
    RETURN OLD;
  END IF;
  RAISE EXCEPTION 'market observations and snapshots are immutable';
END;
$function$;
