//! Shared persistence for "this ESI wallet transaction was recorded into
//! accounting Inventory".
//!
//! The relationship is `inventory_event_sources`: one immutable row per
//! recording, keyed by the purchase event, with `reverted_at` as its lifecycle.
//! Finance and generic ledger reversal both read and write it through here, so
//! there is exactly one truth about whether a transaction is currently in
//! inventory.

use chrono::{DateTime, Utc};
use iskworks_core::{
    FinanceInventoryEvidence, FinanceInventoryLatestRecording, FinanceInventoryRecording,
    InventoryError, InventoryEventId, Money, WorkspaceId,
};
use rust_decimal::Decimal;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Extra select-list columns describing one transaction's recording state.
/// Requires the query to alias `esi_wallet_transactions` as `w` and the SDE
/// type join as `t`, and to include `RECORDING_JOINS`.
pub(crate) const RECORDING_COLUMNS: &str = r#"
               w.is_personal,
               (t.name_en IS NOT NULL) AS type_resolved,
               recording.recording_id,
               recording.recorded_at,
               recording.reverted_at AS recording_reverted_at,
               recording.quantity_delta AS recording_quantity,
               recording.total_cost_delta AS recording_total_basis"#;

/// One batched join (no per-row lookups): the transaction's latest recording,
/// preferring an active one over any reverted history.
pub(crate) const RECORDING_JOINS: &str = r#"
        LEFT JOIN LATERAL (
          SELECT s.inventory_event_id AS recording_id,
                 s.accepted_at AS recorded_at,
                 s.reverted_at,
                 e.quantity_delta,
                 e.total_cost_delta
          FROM inventory_event_sources s
          JOIN inventory_events e ON e.id = s.inventory_event_id
          WHERE s.observation_id = w.id AND s.accounting_effect_kind = 'purchase'
          ORDER BY (s.reverted_at IS NULL) DESC, s.accepted_at DESC
          LIMIT 1
        ) recording ON true"#;

#[derive(sqlx::FromRow)]
pub(crate) struct RecordingColumns {
    is_personal: bool,
    type_resolved: bool,
    recording_id: Option<Uuid>,
    recorded_at: Option<DateTime<Utc>>,
    recording_reverted_at: Option<DateTime<Utc>>,
    recording_quantity: Option<i64>,
    recording_total_basis: Option<Decimal>,
}

impl RecordingColumns {
    pub(crate) fn recording(
        self,
        is_buy: bool,
    ) -> Result<Option<FinanceInventoryRecording>, InventoryError> {
        let latest_recording = match (
            self.recording_id,
            self.recorded_at,
            self.recording_quantity,
            self.recording_total_basis,
        ) {
            (Some(recording_id), Some(recorded_at), Some(quantity), Some(basis)) => {
                Some(FinanceInventoryLatestRecording {
                    recording_id,
                    recorded_at,
                    reverted_at: self.recording_reverted_at,
                    quantity: u64::try_from(quantity)
                        .map_err(|_| InventoryError::InvalidProjection)?,
                    total_basis: Money(basis),
                })
            }
            (None, ..) => None,
            _ => return Err(InventoryError::InvalidProjection),
        };
        Ok(FinanceInventoryRecording::derive(
            FinanceInventoryEvidence {
                is_buy,
                is_personal: self.is_personal,
                type_resolved: self.type_resolved,
                latest_recording,
            },
        ))
    }
}

#[derive(sqlx::FromRow)]
struct StateRow {
    is_buy: bool,
    #[sqlx(flatten)]
    columns: RecordingColumns,
}

/// Current recording state of one wallet transaction, scoped to a workspace
/// through its connection (the transaction table has no workspace column).
/// `None` when the transaction does not exist in this workspace *or* can never
/// be recorded (Market Sell); callers distinguish the two themselves.
pub(crate) async fn recording_state<'e, E>(
    executor: E,
    workspace_id: WorkspaceId,
    observation_id: Uuid,
) -> Result<Option<FinanceInventoryRecording>, InventoryError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let sql = format!(
        r#"
        WITH active_import AS (SELECT id FROM sde_imports WHERE active LIMIT 1)
        SELECT w.is_buy, {RECORDING_COLUMNS}
        FROM esi_wallet_transactions w
        JOIN eve_connections c ON c.id = w.connection_id
        LEFT JOIN active_import ai ON true
        LEFT JOIN sde_types t ON t.import_id = ai.id AND t.type_id = w.type_id
        {RECORDING_JOINS}
        WHERE w.id = $1 AND c.workspace_id = $2
        "#
    );
    let row = sqlx::query_as::<_, StateRow>(&sql)
        .bind(observation_id)
        .bind(workspace_id.0)
        .fetch_optional(executor)
        .await
        .map_err(|error| InventoryError::Persistence(error.to_string()))?
        .ok_or(InventoryError::ItemNotFound)?;
    row.columns.recording(row.is_buy)
}

/// Locks the wallet transaction that `event_id` records, if any. Every path
/// that changes a recording (record, revert, generic ledger reversal) takes
/// this lock *before* any balance lock, so they serialize on the transaction
/// instead of deadlocking on each other.
pub(crate) async fn lock_recorded_transaction(
    tx: &mut Transaction<'_, Postgres>,
    event_id: InventoryEventId,
) -> Result<(), InventoryError> {
    sqlx::query(
        r#"
        SELECT w.id
        FROM esi_wallet_transactions w
        JOIN inventory_event_sources s ON s.observation_id = w.id
        WHERE s.inventory_event_id = $1
        FOR UPDATE OF w
        "#,
    )
    .bind(event_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| InventoryError::Persistence(error.to_string()))?;
    Ok(())
}

/// Marks the recording whose purchase event was just reversed as reverted, so
/// Finance shows the transaction is no longer in inventory. A no-op for events
/// with no wallet provenance. Never deletes history: the recording row stays,
/// and a later re-record adds a new one.
pub(crate) async fn settle_source_reversal(
    tx: &mut Transaction<'_, Postgres>,
    purchase_event_id: InventoryEventId,
    reversal_event_id: InventoryEventId,
) -> Result<(), InventoryError> {
    sqlx::query(
        r#"
        UPDATE inventory_event_sources
        SET reverted_at = now(), reversal_event_id = $2
        WHERE inventory_event_id = $1 AND reverted_at IS NULL
        "#,
    )
    .bind(purchase_event_id.0)
    .bind(reversal_event_id.0)
    .execute(&mut **tx)
    .await
    .map_err(|error| InventoryError::Persistence(error.to_string()))?;
    Ok(())
}
