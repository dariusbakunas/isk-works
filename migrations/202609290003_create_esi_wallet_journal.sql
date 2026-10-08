-- ESI wallet journal (`/characters/{id}/wallet/journal`). ESI only serves about
-- 30 days of it, so it is accumulated from now on. Entries are immutable, so
-- ingestion is insert-if-new keyed on the entry id. Nothing reads this yet;
-- fees/taxes (`brokers_fee`, `transaction_tax`) and true balance history are
-- a later presentation phase.
CREATE TABLE esi_wallet_journal (
  id uuid PRIMARY KEY,
  connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE CASCADE,
  ref_id bigint NOT NULL,
  occurred_at timestamptz NOT NULL,
  ref_type text NOT NULL,
  amount numeric(28,4) NOT NULL,
  balance numeric(28,4),
  first_party_id bigint,
  second_party_id bigint,
  context_id bigint,
  context_id_type text,
  description text,
  reason text,
  tax numeric(28,4),
  tax_receiver_id bigint,
  raw_payload jsonb NOT NULL,
  first_observed_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (connection_id, ref_id)
);
CREATE INDEX esi_wallet_journal_connection_time_idx
  ON esi_wallet_journal (connection_id, occurred_at DESC);
CREATE INDEX esi_wallet_journal_ref_type_idx
  ON esi_wallet_journal (connection_id, ref_type, occurred_at DESC);
