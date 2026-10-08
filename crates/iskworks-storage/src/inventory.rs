use async_trait::async_trait;
use iskworks_core::{
    apply_inventory_event, reversal_posting, CostInputQuality, InventoryBalance, InventoryError,
    InventoryEvent, InventoryEventId, InventoryEventKind, InventoryHistory, InventoryItemKey,
    InventoryPosting, InventoryRepository, Money, MoneyDelta, OwnerId, WorkspaceId,
};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgInventoryRepository {
    pool: PgPool,
}

impl PgInventoryRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The current balance row for one item, or `None` if nothing was ever
    /// posted for it. Not locked.
    pub(crate) async fn balance_with<'e, E>(
        executor: E,
        key: &InventoryItemKey,
    ) -> Result<Option<InventoryBalance>, InventoryError>
    where
        E: sqlx::Executor<'e, Database = Postgres>,
    {
        sqlx::query_as::<_, BalanceRow>(
            r#"
            SELECT workspace_id, owner_id, type_id, captured_name, quantity,
                   total_historical_cost, revision, last_activity_at
            FROM inventory_balances
            WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3
            "#,
        )
        .bind(key.workspace_id.0)
        .bind(key.owner_id.0)
        .bind(key.type_id)
        .fetch_optional(executor)
        .await
        .map_err(map_sqlx)?
        .map(BalanceRow::into_balance)
        .transpose()
    }

    async fn history_with<'e, E>(
        executor: E,
        key: &InventoryItemKey,
    ) -> Result<InventoryHistory, InventoryError>
    where
        E: sqlx::Executor<'e, Database = Postgres> + Copy,
    {
        let balance = sqlx::query_as::<_, BalanceRow>(
            r#"
            SELECT workspace_id, owner_id, type_id, captured_name, quantity,
                   total_historical_cost, revision, last_activity_at
            FROM inventory_balances
            WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3
            "#,
        )
        .bind(key.workspace_id.0)
        .bind(key.owner_id.0)
        .bind(key.type_id)
        .fetch_optional(executor)
        .await
        .map_err(map_sqlx)?
        .ok_or(InventoryError::ItemNotFound)?
        .into_balance()?;

        let rows = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT e.id, e.workspace_id, e.owner_id, e.type_id, e.captured_name,
                   e.event_kind, e.quantity_delta, e.total_cost_delta, e.unit_cost,
                   e.cost_quality, e.source_reference, e.note, e.effective_at,
                   e.recorded_at, e.sequence, e.reverses_event_id,
                   reversed.id AS reversed_by_event_id,
                   e.resulting_quantity, e.resulting_total_cost,
                   e.resulting_average_cost, e.resulting_revision
            FROM inventory_events e
            LEFT JOIN inventory_events reversed ON reversed.reverses_event_id = e.id
            WHERE e.workspace_id = $1 AND e.owner_id = $2 AND e.type_id = $3
            ORDER BY e.sequence ASC
            "#,
        )
        .bind(key.workspace_id.0)
        .bind(key.owner_id.0)
        .bind(key.type_id)
        .fetch_all(executor)
        .await
        .map_err(map_sqlx)?;

        Ok(InventoryHistory {
            balance,
            events: rows
                .into_iter()
                .map(EventRow::into_event)
                .collect::<Result<_, _>>()?,
        })
    }

    pub(crate) async fn post_in_transaction(
        tx: &mut Transaction<'_, Postgres>,
        posting: &InventoryPosting,
    ) -> Result<InventoryBalance, InventoryError> {
        Self::post_in_transaction_with_type_validation(tx, posting, true, false).await
    }

    /// Posts against whatever the balance revision is once the row lock is
    /// held, ignoring `posting.expected_revision`. For server-derived
    /// recordings (a wallet purchase) where there is no user-visible revision
    /// to be stale against: the evidence, not the balance, is authoritative.
    pub(crate) async fn post_at_locked_revision_in_transaction(
        tx: &mut Transaction<'_, Postgres>,
        posting: &InventoryPosting,
    ) -> Result<InventoryBalance, InventoryError> {
        Self::post_in_transaction_with_type_validation(tx, posting, true, true).await
    }

    /// Reverses one specific event inside the caller's transaction: locks the
    /// item's balance, builds the exact compensating event from the stored
    /// original, and posts it. Unlike `reverse_latest` the event need not be
    /// the newest; the ledger's own projection rejects the reversal when the
    /// stock it added has since been used. Historical evidence stays
    /// authoritative, so the active-SDE type check is skipped.
    pub(crate) async fn reverse_event_in_transaction(
        tx: &mut Transaction<'_, Postgres>,
        key: &InventoryItemKey,
        event_id: InventoryEventId,
        reason: String,
    ) -> Result<InventoryEventId, InventoryError> {
        let balance = Self::lock_balance_row(tx, key)
            .await?
            .ok_or(InventoryError::ItemNotFound)?
            .into_balance()?;
        let original = load_event(tx, key, event_id).await?;
        let reversal = reversal_posting(&original, balance.revision, reason)?;
        Self::post_historical_reversal_in_transaction(tx, &reversal).await?;
        Ok(reversal.id)
    }

    /// Applies an exact compensating event built from already-persisted
    /// ledger evidence. Historical evidence remains authoritative even if
    /// the active SDE later renames or removes the type; every other normal
    /// projection, revision, persistence, and uniqueness check still runs.
    pub(crate) async fn post_historical_reversal_in_transaction(
        tx: &mut Transaction<'_, Postgres>,
        posting: &InventoryPosting,
    ) -> Result<InventoryBalance, InventoryError> {
        debug_assert_eq!(posting.kind, InventoryEventKind::Reversal);
        debug_assert!(posting.reverses_event_id.is_some());
        Self::post_in_transaction_with_type_validation(tx, posting, false, false).await
    }

    async fn lock_balance_row(
        tx: &mut Transaction<'_, Postgres>,
        key: &InventoryItemKey,
    ) -> Result<Option<BalanceRow>, InventoryError> {
        sqlx::query_as::<_, BalanceRow>(
            r#"
            SELECT workspace_id, owner_id, type_id, captured_name, quantity,
                   total_historical_cost, revision, last_activity_at
            FROM inventory_balances
            WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3
            FOR UPDATE
            "#,
        )
        .bind(key.workspace_id.0)
        .bind(key.owner_id.0)
        .bind(key.type_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map_sqlx)
    }

    async fn post_in_transaction_with_type_validation(
        tx: &mut Transaction<'_, Postgres>,
        posting: &InventoryPosting,
        validate_active_type: bool,
        use_locked_revision: bool,
    ) -> Result<InventoryBalance, InventoryError> {
        let mut row = Self::lock_balance_row(tx, &posting.key).await?;
        if row.is_none() {
            // A brand-new item has no balance row to lock, so two concurrent
            // first posts would both read revision 0 and the loser would hit
            // the (owner, type, sequence) unique index as an opaque 503.
            // Serialize them on a transaction-scoped advisory lock instead;
            // once it is held the winner's committed row is visible, so the
            // loser sees the real revision (and, for a stale expected
            // revision, a proper RevisionConflict).
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(format!(
                    "inventory-balance:{}:{}:{}",
                    posting.key.workspace_id.0, posting.key.owner_id.0, posting.key.type_id
                ))
                .execute(&mut **tx)
                .await
                .map_err(map_sqlx)?;
            row = Self::lock_balance_row(tx, &posting.key).await?;
        }

        let current = match row {
            Some(row) => row.into_balance()?,
            None => InventoryBalance::empty(posting.key.clone(), posting.type_name.clone()),
        };
        let rebased;
        let posting = if use_locked_revision {
            rebased = InventoryPosting {
                expected_revision: current.revision,
                ..posting.clone()
            };
            &rebased
        } else {
            posting
        };
        if current.revision != posting.expected_revision {
            return Err(InventoryError::RevisionConflict);
        }
        if posting.kind == InventoryEventKind::OpeningBalance {
            let opening_exists: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS(
                  SELECT 1
                  FROM inventory_events opening
                  WHERE opening.workspace_id = $1
                    AND opening.owner_id = $2
                    AND opening.type_id = $3
                    AND opening.event_kind = 'opening_balance'
                    AND NOT EXISTS (
                      SELECT 1
                      FROM inventory_events reversal
                      WHERE reversal.reverses_event_id = opening.id
                    )
                )
                "#,
            )
            .bind(posting.key.workspace_id.0)
            .bind(posting.key.owner_id.0)
            .bind(posting.key.type_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(map_sqlx)?;
            if opening_exists || current.quantity != 0 {
                return Err(InventoryError::OpeningBalanceAlreadyExists);
            }
        }
        let resulting = apply_inventory_event(&current, posting)?;
        if validate_active_type {
            verify_active_type(tx, posting.key.type_id, &posting.type_name).await?;
        }
        insert_event(tx, posting, &resulting).await?;
        upsert_balance(tx, &resulting).await?;
        Ok(resulting)
    }
}

#[async_trait]
impl InventoryRepository for PgInventoryRepository {
    async fn list_balances(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        sqlx::query_as::<_, BalanceRow>(
            r#"
            SELECT workspace_id, owner_id, type_id, captured_name, quantity,
                   total_historical_cost, revision, last_activity_at
            FROM inventory_balances
            WHERE workspace_id = $1 AND owner_id = $2
            ORDER BY captured_name ASC
            "#,
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(BalanceRow::into_balance)
        .collect()
    }

    async fn get_history(
        &self,
        key: &InventoryItemKey,
    ) -> Result<InventoryHistory, InventoryError> {
        Self::history_with(&self.pool, key).await
    }

    async fn list_events_by_type(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, Vec<InventoryEvent>>, InventoryError> {
        if type_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        // `history_with`'s event query, for many types at once.
        let rows = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT e.id, e.workspace_id, e.owner_id, e.type_id, e.captured_name,
                   e.event_kind, e.quantity_delta, e.total_cost_delta, e.unit_cost,
                   e.cost_quality, e.source_reference, e.note, e.effective_at,
                   e.recorded_at, e.sequence, e.reverses_event_id,
                   reversed.id AS reversed_by_event_id,
                   e.resulting_quantity, e.resulting_total_cost,
                   e.resulting_average_cost, e.resulting_revision
            FROM inventory_events e
            LEFT JOIN inventory_events reversed ON reversed.reverses_event_id = e.id
            WHERE e.workspace_id = $1 AND e.owner_id = $2 AND e.type_id = ANY($3)
            ORDER BY e.type_id ASC, e.sequence ASC
            "#,
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        let mut events: std::collections::BTreeMap<i64, Vec<InventoryEvent>> =
            std::collections::BTreeMap::new();
        for row in rows {
            let event = row.into_event()?;
            events.entry(event.key.type_id).or_default().push(event);
        }
        Ok(events)
    }

    async fn post(&self, posting: InventoryPosting) -> Result<InventoryHistory, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        Self::post_in_transaction(&mut tx, &posting).await?;
        tx.commit().await.map_err(map_sqlx)?;
        Self::history_with(&self.pool, &posting.key).await
    }

    async fn reverse_latest(
        &self,
        key: &InventoryItemKey,
        event_id: InventoryEventId,
        expected_revision: u64,
        reason: String,
    ) -> Result<InventoryHistory, InventoryError> {
        if reason.trim().is_empty() {
            return Err(InventoryError::Validation(
                "A reversal reason is required.".to_string(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        // Provenance before balance, the same order a provenance-aware
        // reversal takes, so the two can never deadlock on each other.
        crate::wallet_recording::lock_recorded_transaction(&mut tx, event_id).await?;
        sqlx::query(
            "SELECT revision FROM inventory_balances WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 FOR UPDATE",
        )
        .bind(key.workspace_id.0)
        .bind(key.owner_id.0)
        .bind(key.type_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?
        .ok_or(InventoryError::ItemNotFound)?;

        let latest_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id FROM inventory_events
            WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3
            ORDER BY sequence DESC LIMIT 1
            "#,
        )
        .bind(key.workspace_id.0)
        .bind(key.owner_id.0)
        .bind(key.type_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        if latest_id != event_id.0 {
            return Err(InventoryError::LatestEventOnly);
        }
        let original = load_event(&mut tx, key, event_id).await?;
        let reversal = reversal_posting(&original, expected_revision, reason)?;
        Self::post_in_transaction(&mut tx, &reversal).await?;
        // A sourced purchase (e.g. a recorded wallet transaction) must not keep
        // claiming to be active once its event is reversed.
        crate::wallet_recording::settle_source_reversal(&mut tx, original.id, reversal.id).await?;
        tx.commit().await.map_err(map_sqlx)?;
        Self::history_with(&self.pool, key).await
    }

    async fn rebuild(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        let keys = sqlx::query_as::<_, KeyRow>(
            r#"
            SELECT DISTINCT workspace_id, owner_id, type_id
            FROM inventory_events
            WHERE workspace_id = $1 AND owner_id = $2
            ORDER BY type_id
            "#,
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        let mut rebuilt = Vec::with_capacity(keys.len());
        for row in keys {
            let key = row.into_key();
            let history = Self::history_with(&self.pool, &key).await?;
            let mut balance =
                InventoryBalance::empty(key.clone(), history.balance.type_name.clone());
            for event in &history.events {
                let posting = event_as_posting(event, balance.revision);
                balance = apply_inventory_event(&balance, &posting)?;
            }
            if balance != history.balance {
                let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
                upsert_balance(&mut tx, &balance).await?;
                tx.commit().await.map_err(map_sqlx)?;
            }
            rebuilt.push(balance);
        }
        Ok(rebuilt)
    }
}

async fn verify_active_type(
    tx: &mut Transaction<'_, Postgres>,
    type_id: i64,
    type_name: &str,
) -> Result<(), InventoryError> {
    let actual = sqlx::query_scalar::<_, String>(
        r#"
        SELECT t.name_en
        FROM sde_types t
        JOIN sde_imports i ON i.id = t.import_id AND i.active
        WHERE t.type_id = $1
        "#,
    )
    .bind(type_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_sqlx)?
    .ok_or_else(|| {
        InventoryError::Validation("The EVE type is not in the active SDE.".to_string())
    })?;
    if actual != type_name {
        return Err(InventoryError::Validation(
            "The EVE item name does not match the active SDE.".to_string(),
        ));
    }
    Ok(())
}

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    posting: &InventoryPosting,
    resulting: &InventoryBalance,
) -> Result<(), InventoryError> {
    sqlx::query(
        r#"
        INSERT INTO inventory_events (
          id, workspace_id, owner_id, type_id, captured_name, event_kind,
          quantity_delta, total_cost_delta, unit_cost, cost_quality,
          source_reference, note, effective_at, recorded_at, sequence,
          reverses_event_id, resulting_quantity, resulting_total_cost,
          resulting_average_cost, resulting_revision
        ) VALUES (
          $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20
        )
        "#,
    )
    .bind(posting.id.0)
    .bind(posting.key.workspace_id.0)
    .bind(posting.key.owner_id.0)
    .bind(posting.key.type_id)
    .bind(&posting.type_name)
    .bind(kind_to_str(posting.kind))
    .bind(posting.quantity_delta)
    .bind(posting.total_cost_delta.0)
    .bind(posting.unit_cost.map(|money| money.0))
    .bind(quality_to_str(posting.cost_quality))
    .bind(&posting.source_reference)
    .bind(&posting.note)
    .bind(posting.effective_at)
    .bind(posting.recorded_at)
    .bind(i64::try_from(resulting.revision).map_err(|_| InventoryError::ArithmeticOverflow)?)
    .bind(posting.reverses_event_id.map(|id| id.0))
    .bind(i64::try_from(resulting.quantity).map_err(|_| InventoryError::ArithmeticOverflow)?)
    .bind(resulting.total_historical_cost.0)
    .bind(resulting.average_unit_cost.map(|money| money.0))
    .bind(i64::try_from(resulting.revision).map_err(|_| InventoryError::ArithmeticOverflow)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| {
        if posting.reverses_event_id.is_some()
            && error
                .as_database_error()
                .is_some_and(|database| database.code().as_deref() == Some("23505"))
        {
            InventoryError::AlreadyReversed
        } else {
            map_sqlx(error)
        }
    })?;
    Ok(())
}

async fn upsert_balance(
    tx: &mut Transaction<'_, Postgres>,
    balance: &InventoryBalance,
) -> Result<(), InventoryError> {
    sqlx::query(
        r#"
        INSERT INTO inventory_balances (
          workspace_id, owner_id, type_id, captured_name, quantity,
          total_historical_cost, revision, last_activity_at
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
        ON CONFLICT (workspace_id, owner_id, type_id) DO UPDATE SET
          captured_name = EXCLUDED.captured_name,
          quantity = EXCLUDED.quantity,
          total_historical_cost = EXCLUDED.total_historical_cost,
          revision = EXCLUDED.revision,
          last_activity_at = EXCLUDED.last_activity_at
        "#,
    )
    .bind(balance.key.workspace_id.0)
    .bind(balance.key.owner_id.0)
    .bind(balance.key.type_id)
    .bind(&balance.type_name)
    .bind(i64::try_from(balance.quantity).map_err(|_| InventoryError::ArithmeticOverflow)?)
    .bind(balance.total_historical_cost.0)
    .bind(i64::try_from(balance.revision).map_err(|_| InventoryError::ArithmeticOverflow)?)
    .bind(
        balance
            .last_activity_at
            .ok_or(InventoryError::InvalidProjection)?,
    )
    .execute(&mut **tx)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

async fn load_event(
    tx: &mut Transaction<'_, Postgres>,
    key: &InventoryItemKey,
    id: InventoryEventId,
) -> Result<InventoryEvent, InventoryError> {
    sqlx::query_as::<_, EventRow>(
        r#"
        SELECT e.id, e.workspace_id, e.owner_id, e.type_id, e.captured_name,
               e.event_kind, e.quantity_delta, e.total_cost_delta, e.unit_cost,
               e.cost_quality, e.source_reference, e.note, e.effective_at,
               e.recorded_at, e.sequence, e.reverses_event_id,
               reversed.id AS reversed_by_event_id,
               e.resulting_quantity, e.resulting_total_cost,
               e.resulting_average_cost, e.resulting_revision
        FROM inventory_events e
        LEFT JOIN inventory_events reversed ON reversed.reverses_event_id = e.id
        WHERE e.id = $1 AND e.workspace_id = $2 AND e.owner_id = $3 AND e.type_id = $4
        "#,
    )
    .bind(id.0)
    .bind(key.workspace_id.0)
    .bind(key.owner_id.0)
    .bind(key.type_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_sqlx)?
    .ok_or(InventoryError::EventNotFound)?
    .into_event()
}

fn event_as_posting(event: &InventoryEvent, expected_revision: u64) -> InventoryPosting {
    InventoryPosting {
        id: event.id,
        key: event.key.clone(),
        type_name: event.type_name.clone(),
        kind: event.kind,
        quantity_delta: event.quantity_delta,
        total_cost_delta: event.total_cost_delta,
        unit_cost: event.unit_cost,
        cost_quality: event.cost_quality,
        source_reference: event.source_reference.clone(),
        note: event.note.clone(),
        effective_at: event.effective_at,
        recorded_at: event.recorded_at,
        expected_revision,
        reverses_event_id: event.reverses_event_id,
    }
}

#[derive(sqlx::FromRow)]
struct BalanceRow {
    workspace_id: Uuid,
    owner_id: Uuid,
    type_id: i64,
    captured_name: String,
    quantity: i64,
    total_historical_cost: Decimal,
    revision: i64,
    last_activity_at: chrono::DateTime<chrono::Utc>,
}

impl BalanceRow {
    fn into_balance(self) -> Result<InventoryBalance, InventoryError> {
        let quantity =
            u64::try_from(self.quantity).map_err(|_| InventoryError::InvalidProjection)?;
        let average_unit_cost = if quantity > 0 {
            let mut average = self
                .total_historical_cost
                .checked_div(Decimal::from_i128_with_scale(i128::from(quantity), 0))
                .ok_or(InventoryError::ArithmeticOverflow)?;
            average.rescale(4);
            Some(Money(average))
        } else {
            None
        };
        Ok(InventoryBalance {
            key: InventoryItemKey {
                workspace_id: WorkspaceId(self.workspace_id),
                owner_id: OwnerId(self.owner_id),
                type_id: self.type_id,
            },
            type_name: self.captured_name,
            quantity,
            total_historical_cost: Money(self.total_historical_cost),
            average_unit_cost,
            revision: u64::try_from(self.revision)
                .map_err(|_| InventoryError::InvalidProjection)?,
            last_activity_at: Some(self.last_activity_at),
        })
    }
}

#[derive(sqlx::FromRow)]
struct EventRow {
    id: Uuid,
    workspace_id: Uuid,
    owner_id: Uuid,
    type_id: i64,
    captured_name: String,
    event_kind: String,
    quantity_delta: i64,
    total_cost_delta: Decimal,
    unit_cost: Option<Decimal>,
    cost_quality: String,
    source_reference: String,
    note: String,
    effective_at: chrono::DateTime<chrono::Utc>,
    recorded_at: chrono::DateTime<chrono::Utc>,
    sequence: i64,
    reverses_event_id: Option<Uuid>,
    reversed_by_event_id: Option<Uuid>,
    resulting_quantity: i64,
    resulting_total_cost: Decimal,
    resulting_average_cost: Option<Decimal>,
    resulting_revision: i64,
}

impl EventRow {
    fn into_event(self) -> Result<InventoryEvent, InventoryError> {
        Ok(InventoryEvent {
            id: InventoryEventId(self.id),
            key: InventoryItemKey {
                workspace_id: WorkspaceId(self.workspace_id),
                owner_id: OwnerId(self.owner_id),
                type_id: self.type_id,
            },
            type_name: self.captured_name.clone(),
            kind: kind_from_str(&self.event_kind)?,
            quantity_delta: self.quantity_delta,
            total_cost_delta: MoneyDelta(self.total_cost_delta),
            unit_cost: self.unit_cost.map(Money),
            cost_quality: quality_from_str(&self.cost_quality)?,
            source_reference: self.source_reference,
            note: self.note,
            effective_at: self.effective_at,
            recorded_at: self.recorded_at,
            sequence: u64::try_from(self.sequence)
                .map_err(|_| InventoryError::InvalidProjection)?,
            reverses_event_id: self.reverses_event_id.map(InventoryEventId),
            reversed_by_event_id: self.reversed_by_event_id.map(InventoryEventId),
            resulting_balance: InventoryBalance {
                key: InventoryItemKey {
                    workspace_id: WorkspaceId(self.workspace_id),
                    owner_id: OwnerId(self.owner_id),
                    type_id: self.type_id,
                },
                type_name: self.captured_name,
                quantity: u64::try_from(self.resulting_quantity)
                    .map_err(|_| InventoryError::InvalidProjection)?,
                total_historical_cost: Money(self.resulting_total_cost),
                average_unit_cost: self.resulting_average_cost.map(Money),
                revision: u64::try_from(self.resulting_revision)
                    .map_err(|_| InventoryError::InvalidProjection)?,
                last_activity_at: Some(self.recorded_at),
            },
        })
    }
}

#[derive(sqlx::FromRow)]
struct KeyRow {
    workspace_id: Uuid,
    owner_id: Uuid,
    type_id: i64,
}

impl KeyRow {
    fn into_key(self) -> InventoryItemKey {
        InventoryItemKey {
            workspace_id: WorkspaceId(self.workspace_id),
            owner_id: OwnerId(self.owner_id),
            type_id: self.type_id,
        }
    }
}

fn kind_to_str(kind: InventoryEventKind) -> &'static str {
    match kind {
        InventoryEventKind::OpeningBalance => "opening_balance",
        InventoryEventKind::Purchase => "purchase",
        InventoryEventKind::Consumption => "consumption",
        InventoryEventKind::ProductionOutput => "production_output",
        InventoryEventKind::Reversal => "reversal",
        InventoryEventKind::Adjustment => "adjustment",
    }
}

fn kind_from_str(value: &str) -> Result<InventoryEventKind, InventoryError> {
    match value {
        "opening_balance" => Ok(InventoryEventKind::OpeningBalance),
        "purchase" => Ok(InventoryEventKind::Purchase),
        "consumption" => Ok(InventoryEventKind::Consumption),
        "production_output" => Ok(InventoryEventKind::ProductionOutput),
        "reversal" => Ok(InventoryEventKind::Reversal),
        "adjustment" => Ok(InventoryEventKind::Adjustment),
        _ => Err(InventoryError::InvalidProjection),
    }
}

fn quality_to_str(quality: CostInputQuality) -> &'static str {
    match quality {
        CostInputQuality::Known => "known",
        CostInputQuality::Estimated => "estimated",
        CostInputQuality::ZeroCost => "zero_cost",
    }
}

fn quality_from_str(value: &str) -> Result<CostInputQuality, InventoryError> {
    match value {
        "known" => Ok(CostInputQuality::Known),
        "estimated" => Ok(CostInputQuality::Estimated),
        "zero_cost" => Ok(CostInputQuality::ZeroCost),
        _ => Err(InventoryError::InvalidProjection),
    }
}

fn map_sqlx(error: sqlx::Error) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iskworks_core::{InventoryRepository, Money};

    async fn fixture(pool: &PgPool) -> (InventoryItemKey, PgInventoryRepository) {
        let workspace_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let import_id = Uuid::new_v4();
        let now = crate::db_now();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) VALUES ($1, 'Inventory Test', $2, $3, $3)",
        )
        .bind(workspace_id)
        .bind(owner_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) VALUES ($1, $2, 'manual', 'Inventory Test', true, $3, $3)",
        )
        .bind(owner_id)
        .bind(workspace_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sde_imports (id, source_version, source_label, source_checksum, status, active, started_at, completed_at) VALUES ($1, 'test', 'fixture', 'inventory-fixture', 'active', true, $2, $2)",
        )
        .bind(import_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sde_types (import_id, type_id, name_en, published) VALUES ($1, 34, 'Tritanium', true)",
        )
        .bind(import_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        (
            InventoryItemKey {
                workspace_id: WorkspaceId(workspace_id),
                owner_id: OwnerId(owner_id),
                type_id: 34,
            },
            PgInventoryRepository::new(pool.clone()),
        )
    }

    fn posting(
        key: &InventoryItemKey,
        revision: u64,
        quantity: i64,
        unit_cost: &str,
        kind: InventoryEventKind,
    ) -> InventoryPosting {
        let unit_cost = Money::parse(unit_cost).unwrap();
        InventoryPosting {
            id: InventoryEventId::new(),
            key: key.clone(),
            type_name: "Tritanium".to_string(),
            kind,
            quantity_delta: quantity,
            total_cost_delta: MoneyDelta(
                unit_cost
                    .checked_mul_quantity(quantity.unsigned_abs())
                    .unwrap()
                    .0,
            ),
            unit_cost: Some(unit_cost),
            cost_quality: CostInputQuality::Known,
            source_reference: "fixture".to_string(),
            note: String::new(),
            effective_at: crate::db_now(),
            recorded_at: crate::db_now(),
            expected_revision: revision,
            reverses_event_id: None,
        }
    }

    /// The Inventory list's batched event read returns, per type, exactly
    /// what `get_history` returns as that type's events -- order, reversal
    /// links and all -- and omits a type with no events.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn list_events_by_type_matches_get_history_per_type(pool: PgPool) {
        let (key, repository) = fixture(&pool).await;
        let active_import: Uuid =
            sqlx::query_scalar("SELECT id FROM sde_imports WHERE active = true")
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO sde_types (import_id, type_id, name_en, published) VALUES ($1, 35, 'Pyerite', true)",
        )
        .bind(active_import)
        .execute(&pool)
        .await
        .unwrap();
        let pyerite = InventoryItemKey {
            type_id: 35,
            ..key.clone()
        };
        repository
            .post(posting(
                &key,
                0,
                100,
                "10.0000",
                InventoryEventKind::OpeningBalance,
            ))
            .await
            .unwrap();
        let purchase = repository
            .post(posting(
                &key,
                1,
                50,
                "16.0000",
                InventoryEventKind::Purchase,
            ))
            .await
            .unwrap();
        repository
            .reverse_latest(&key, purchase.events[1].id, 2, "typo".to_string())
            .await
            .unwrap();
        let mut pyerite_opening =
            posting(&pyerite, 0, 7, "2.0000", InventoryEventKind::OpeningBalance);
        pyerite_opening.type_name = "Pyerite".to_string();
        repository.post(pyerite_opening).await.unwrap();

        let batched = repository
            .list_events_by_type(key.workspace_id, key.owner_id, &[35, 34, 36])
            .await
            .unwrap();
        let tritanium = repository.get_history(&key).await.unwrap().events;
        assert_eq!(tritanium.len(), 3);
        assert!(tritanium[1].reversed_by_event_id.is_some());
        assert_eq!(
            batched,
            std::collections::BTreeMap::from([
                (34, tritanium),
                (35, repository.get_history(&pyerite).await.unwrap().events),
            ])
        );
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn events_and_projection_commit_with_exact_weighted_average(pool: PgPool) {
        let (key, repository) = fixture(&pool).await;
        repository
            .post(posting(
                &key,
                0,
                100,
                "10.0000",
                InventoryEventKind::OpeningBalance,
            ))
            .await
            .unwrap();
        let history = repository
            .post(posting(
                &key,
                1,
                50,
                "16.0000",
                InventoryEventKind::Purchase,
            ))
            .await
            .unwrap();
        assert_eq!(history.events.len(), 2);
        assert_eq!(history.balance.quantity, 150);
        assert_eq!(
            history.balance.average_unit_cost,
            Some(Money::parse("12.0000").unwrap())
        );
        assert_eq!(
            history.balance.total_historical_cost,
            Money::parse("1800.0000").unwrap()
        );

        let stale = repository
            .post(posting(&key, 1, 1, "1.0000", InventoryEventKind::Purchase))
            .await;
        assert!(matches!(stale, Err(InventoryError::RevisionConflict)));
        let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM inventory_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(event_count, 2);
    }

    /// Regression test for a real bug found during a manual walkthrough:
    /// an earlier migration rewrote
    /// `inventory_events_kind_valid` to only allow `opening_balance`/
    /// `purchase`/`consumption`/`reversal` -- at the time nothing
    /// created a `production_output` event. `POST
    /// /api/plans/:id/post-output` is the first feature to
    /// actually insert one; without the fix
    /// (`202608140002_allow_production_output_inventory_events.sql`)
    /// this fails with a CHECK constraint violation.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn production_output_events_are_accepted_by_the_kind_check_constraint(pool: PgPool) {
        let (key, repository) = fixture(&pool).await;

        let history = repository
            .post(posting(
                &key,
                0,
                2,
                "350.0000",
                InventoryEventKind::ProductionOutput,
            ))
            .await
            .unwrap();

        assert_eq!(history.balance.quantity, 2);
        assert_eq!(
            history.balance.total_historical_cost,
            Money::parse("700.0000").unwrap()
        );
    }

    /// Same shape as `production_output_events_are_accepted_by_the_kind_check_constraint`:
    /// proves `Adjustment` round-trips through the real
    /// `post_in_transaction` -> `insert_event`/`upsert_balance` path --
    /// the CHECK constraint accepts it, `kind_to_str`/`kind_from_str`
    /// round-trip it correctly, and no second mutation path was needed.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn adjustment_events_are_accepted_by_the_kind_check_constraint(pool: PgPool) {
        let (key, repository) = fixture(&pool).await;

        let history = repository
            .post(posting(
                &key,
                0,
                49,
                "4.0000",
                InventoryEventKind::Adjustment,
            ))
            .await
            .unwrap();

        assert_eq!(history.events.len(), 1);
        assert_eq!(history.events[0].kind, InventoryEventKind::Adjustment);
        assert_eq!(history.balance.quantity, 49);
        assert_eq!(
            history.balance.total_historical_cost,
            Money::parse("196.0000").unwrap()
        );
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn latest_event_reversal_is_append_only_and_rebuilds(pool: PgPool) {
        let (key, repository) = fixture(&pool).await;
        let opening = repository
            .post(posting(
                &key,
                0,
                100,
                "10.0000",
                InventoryEventKind::OpeningBalance,
            ))
            .await
            .unwrap();
        let purchase = repository
            .post(posting(
                &key,
                1,
                50,
                "16.0000",
                InventoryEventKind::Purchase,
            ))
            .await
            .unwrap();
        assert!(matches!(
            repository
                .reverse_latest(&key, opening.events[0].id, 2, "Wrong opening".to_string())
                .await,
            Err(InventoryError::LatestEventOnly)
        ));
        let reversed = repository
            .reverse_latest(
                &key,
                purchase.events[1].id,
                2,
                "Duplicate purchase".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(reversed.events.len(), 3);
        assert_eq!(reversed.balance.quantity, 100);
        assert_eq!(
            reversed.balance.total_historical_cost,
            Money::parse("1000.0000").unwrap()
        );
        assert_eq!(
            reversed.events[1].reversed_by_event_id,
            Some(reversed.events[2].id)
        );

        let rebuilt = repository
            .rebuild(key.workspace_id, key.owner_id)
            .await
            .unwrap();
        assert_eq!(rebuilt, vec![reversed.balance]);
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn first_opening_is_allowed_after_fully_reversed_prior_activity(pool: PgPool) {
        let (key, repository) = fixture(&pool).await;
        let purchase = repository
            .post(posting(
                &key,
                0,
                100,
                "4.2500",
                InventoryEventKind::Purchase,
            ))
            .await
            .unwrap();
        repository
            .reverse_latest(
                &key,
                purchase.events[0].id,
                1,
                "Reversed fixture purchase".to_string(),
            )
            .await
            .unwrap();
        let opening = repository
            .post(posting(
                &key,
                2,
                500,
                "3.6900",
                InventoryEventKind::OpeningBalance,
            ))
            .await
            .unwrap();
        assert_eq!(opening.balance.quantity, 500);

        assert!(matches!(
            repository
                .post(posting(
                    &key,
                    3,
                    1,
                    "3.6900",
                    InventoryEventKind::OpeningBalance,
                ))
                .await,
            Err(InventoryError::OpeningBalanceAlreadyExists)
        ));

        repository
            .reverse_latest(
                &key,
                opening.events[2].id,
                3,
                "Incorrect opening quantity".to_string(),
            )
            .await
            .unwrap();
        let replacement = repository
            .post(posting(
                &key,
                4,
                750,
                "3.6900",
                InventoryEventKind::OpeningBalance,
            ))
            .await
            .unwrap();
        assert_eq!(replacement.balance.quantity, 750);
    }
}
