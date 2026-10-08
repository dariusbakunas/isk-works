use super::sync_runs::{checksum, finalize_run};
use super::*;

impl PgEsiRepository {
    pub async fn unresolved_wallet_client_ids(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<i64>, InventoryError> {
        sqlx::query_scalar(
            r#"
            SELECT DISTINCT transaction.client_id
            FROM esi_wallet_transactions transaction
            JOIN eve_connections connection ON connection.id=transaction.connection_id
            LEFT JOIN eve_entity_names entity ON entity.entity_id=transaction.client_id
            LEFT JOIN eve_entity_name_misses miss
              ON miss.entity_id=transaction.client_id
             AND miss.checked_at > now() - interval '7 days'
            WHERE connection.workspace_id=$1
              AND connection.disconnected_at IS NULL
              AND entity.entity_id IS NULL
              AND miss.entity_id IS NULL
            ORDER BY transaction.client_id
            "#,
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    /// Records IDs `/universe/names` could not name, so wallet enrichment
    /// skips them for a week instead of resending them every sync.
    pub async fn record_entity_name_misses(&self, ids: &[i64]) -> Result<(), InventoryError> {
        if ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r#"INSERT INTO eve_entity_name_misses (entity_id,checked_at)
               SELECT entity_id, now() FROM unnest($1::bigint[]) AS ids(entity_id)
               WHERE entity_id > 0
               ON CONFLICT (entity_id) DO UPDATE SET checked_at=EXCLUDED.checked_at"#,
        )
        .bind(ids)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_sqlx)
    }

    pub async fn cache_entity_names(&self, names: &[EveEntityName]) -> Result<(), InventoryError> {
        if names.is_empty() {
            return Ok(());
        }
        let mut transaction = self.pool.begin().await.map_err(map_sqlx)?;
        for name in names {
            sqlx::query(
                r#"
                INSERT INTO eve_entity_names (entity_id,entity_name,category,observed_at)
                VALUES ($1,$2,$3,$4)
                ON CONFLICT (entity_id) DO UPDATE SET
                  entity_name=EXCLUDED.entity_name,
                  category=EXCLUDED.category,
                  observed_at=EXCLUDED.observed_at
                "#,
            )
            .bind(name.id)
            .bind(&name.name)
            .bind(&name.category)
            .bind(crate::db_now())
            .execute(&mut *transaction)
            .await
            .map_err(map_sqlx)?;
        }
        transaction.commit().await.map_err(map_sqlx)
    }

    pub async fn complete_wallet(
        &self,
        run: &EsiSyncRun,
        observations: &[WalletTransactionObservation],
        etag: Option<&str>,
    ) -> Result<SyncCompletion, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        // Serialize completion of this run.
        sqlx::query("SELECT id FROM esi_sync_runs WHERE id = $1 FOR UPDATE")
            .bind(run.id.0)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        let mut imported = 0_u64;
        let mut unchanged = 0_u64;
        let mut last_id = None;
        for observation in observations {
            last_id = Some(last_id.map_or(observation.transaction_id, |value: i64| {
                value.max(observation.transaction_id)
            }));
            let payload = serde_json::to_value(observation).map_err(map_json)?;
            let observation_id = Uuid::new_v4();
            let total = observation
                .unit_price
                .checked_mul(Decimal::from(observation.quantity))
                .ok_or(InventoryError::ArithmeticOverflow)?;
            let row = sqlx::query_scalar::<_, Uuid>(
                r#"
                INSERT INTO esi_wallet_transactions (
                  id, connection_id, source_transaction_id, first_sync_run_id, last_sync_run_id,
                  type_id, quantity, unit_price, total_price, is_buy, is_personal,
                  transacted_at, location_id, client_id, journal_ref_id, raw_payload,
                  source_checksum, first_observed_at, last_observed_at
                ) VALUES ($1,$2,$3,$4,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now(),now())
                ON CONFLICT (connection_id, source_transaction_id) DO UPDATE SET
                  last_sync_run_id = EXCLUDED.last_sync_run_id,
                  last_observed_at = now()
                RETURNING id
                "#,
            )
            .bind(observation_id)
            .bind(run.connection_id.0)
            .bind(observation.transaction_id)
            .bind(run.id.0)
            .bind(observation.type_id)
            .bind(observation.quantity)
            .bind(observation.unit_price)
            .bind(total)
            .bind(observation.is_buy)
            .bind(observation.is_personal)
            .bind(observation.transacted_at)
            .bind(observation.location_id)
            .bind(observation.client_id)
            .bind(observation.journal_ref_id)
            .bind(&payload)
            .bind(checksum(&payload))
            .fetch_one(&mut *tx)
            .await
            .map_err(map_sqlx)?;
            if row == observation_id {
                imported += 1;
            } else {
                unchanged += 1;
            }
        }
        sqlx::query(
            r#"
            INSERT INTO esi_sync_checkpoints (
              connection_id, endpoint_kind, last_successful_sync_at, last_transaction_id, etag
            ) VALUES ($1,'wallet_transactions',now(),$2,$3)
            ON CONFLICT (connection_id, endpoint_kind) DO UPDATE SET
              last_successful_sync_at = now(),
              last_transaction_id = GREATEST(esi_sync_checkpoints.last_transaction_id, EXCLUDED.last_transaction_id),
              etag = EXCLUDED.etag
            "#,
        )
        .bind(run.connection_id.0)
        .bind(last_id)
        .bind(etag)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        finalize_run(
            &mut tx,
            run.id,
            imported,
            unchanged,
            "Wallet transactions synchronized.",
        )
        .await?;
        tx.commit().await.map_err(map_sqlx)?;
        Ok(SyncCompletion {
            imported,
            unchanged,
        })
    }

    /// Insert journal entries not seen before; existing entries are left
    /// untouched (journal rows are immutable). Returns how many were new.
    pub async fn save_wallet_journal(
        &self,
        connection_id: ConnectedCharacterId,
        entries: &[WalletJournalObservation],
    ) -> Result<u64, InventoryError> {
        let mut inserted = 0_u64;
        for chunk in entries.chunks(500) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO esi_wallet_journal (id, connection_id, ref_id, occurred_at, ref_type, \
                 amount, balance, first_party_id, second_party_id, context_id, context_id_type, \
                 description, reason, tax, tax_receiver_id, raw_payload) ",
            );
            query.push_values(chunk, |mut row, entry| {
                row.push_bind(Uuid::new_v4())
                    .push_bind(connection_id.0)
                    .push_bind(entry.ref_id)
                    .push_bind(entry.date)
                    .push_bind(&entry.ref_type)
                    .push_bind(entry.amount)
                    .push_bind(entry.balance)
                    .push_bind(entry.first_party_id)
                    .push_bind(entry.second_party_id)
                    .push_bind(entry.context_id)
                    .push_bind(&entry.context_id_type)
                    .push_bind(&entry.description)
                    .push_bind(&entry.reason)
                    .push_bind(entry.tax)
                    .push_bind(entry.tax_receiver_id)
                    .push_bind(&entry.raw);
            });
            query.push(" ON CONFLICT (connection_id, ref_id) DO NOTHING");
            inserted += query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sqlx)?
                .rows_affected();
        }
        Ok(inserted)
    }

    pub async fn save_wallet_balance(
        &self,
        run: &EsiSyncRun,
        balance: Decimal,
        observed_at: DateTime<Utc>,
        source_checksum: &str,
    ) -> Result<(), InventoryError> {
        sqlx::query(
            r#"
            INSERT INTO esi_wallet_balances (
              id, connection_id, sync_run_id, balance, observed_at, source_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6)
            ON CONFLICT (connection_id, sync_run_id) DO UPDATE SET
              balance=EXCLUDED.balance,
              observed_at=EXCLUDED.observed_at,
              source_checksum=EXCLUDED.source_checksum
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(run.connection_id.0)
        .bind(run.id.0)
        .bind(balance)
        .bind(observed_at)
        .bind(source_checksum)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }
}
