use iskworks_core::{
    apply_inventory_event, exact_total, purchase_posting, CostInputQuality,
    FinanceInventoryRecording, InventoryBalance, InventoryItemKey, InventoryPreview,
    PostInventoryCommand, WorkspaceId,
};

use super::*;
use crate::wallet_recording::{lock_recorded_transaction, recording_state, settle_source_reversal};

/// Identity of the exact wallet transaction a purchase event records.
pub(super) struct PurchaseSource {
    pub workspace_id: Uuid,
    pub owner_id: Uuid,
    pub observation_id: Uuid,
    pub connection_id: Uuid,
    pub source_transaction_id: i64,
    pub sync_run_id: Uuid,
    pub transacted_at: DateTime<Utc>,
}

/// The one place a wallet purchase becomes ledger state: post the purchase
/// event and write its provenance row, in the caller's transaction, with the
/// wallet transaction row already locked.
async fn record_purchase_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    source: &PurchaseSource,
    posting: &InventoryPosting,
    at_locked_revision: bool,
) -> Result<(), InventoryError> {
    if at_locked_revision {
        PgInventoryRepository::post_at_locked_revision_in_transaction(tx, posting).await?;
    } else {
        PgInventoryRepository::post_in_transaction(tx, posting).await?;
    }
    sqlx::query(
        r#"
        INSERT INTO inventory_event_sources (
          inventory_event_id, workspace_id, owner_id, source_system,
          source_record_kind, source_record_id, accounting_effect_kind,
          connection_id, observation_id, sync_run_id, source_transaction_at, accepted_at
        ) VALUES ($1,$2,$3,'esi','wallet_transaction',$4,'purchase',$5,$6,$7,$8,now())
        "#,
    )
    .bind(posting.id.0)
    .bind(source.workspace_id)
    .bind(source.owner_id)
    .bind(source.source_transaction_id.to_string())
    .bind(source.connection_id)
    .bind(source.observation_id)
    .bind(source.sync_run_id)
    .bind(source.transacted_at)
    .execute(&mut **tx)
    .await
    .map_err(map_sqlx)?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct RecordingTargetRow {
    connection_id: Uuid,
    owner_id: Uuid,
    source_transaction_id: i64,
    first_sync_run_id: Uuid,
    type_id: i64,
    quantity: i64,
    unit_price: Decimal,
    total_price: Decimal,
    is_buy: bool,
    is_personal: bool,
    transacted_at: DateTime<Utc>,
}

impl PgEsiRepository {
    /// Records one Market Buy into accounting Inventory using only persisted
    /// evidence: type, quantity and `unit_price` come from the stored wallet
    /// transaction, the owner from its authorized connection. Idempotent: if
    /// the transaction is already actively recorded (double click, second
    /// tab) nothing is posted and the existing recording is returned with
    /// `created == false`.
    pub async fn record_wallet_purchase(
        &self,
        workspace_id: WorkspaceId,
        observation_id: Uuid,
    ) -> Result<(FinanceInventoryRecording, bool), InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        // Locking the wallet transaction row is what serializes every attempt
        // to record or revert it.
        let target = load_recording_target(&mut tx, workspace_id, observation_id, true).await?;
        if ensure_recordable(&target)? {
            tx.rollback().await.map_err(map_sqlx)?;
            return Ok((
                self.current_recording(workspace_id, observation_id).await?,
                false,
            ));
        }
        let posting = wallet_purchase_posting(&mut tx, workspace_id, &target.row).await?;
        record_purchase_in_transaction(
            &mut tx,
            &PurchaseSource {
                workspace_id: workspace_id.0,
                owner_id: target.row.owner_id,
                observation_id,
                connection_id: target.row.connection_id,
                source_transaction_id: target.row.source_transaction_id,
                sync_run_id: target.row.first_sync_run_id,
                transacted_at: target.row.transacted_at,
            },
            &posting,
            true,
        )
        .await?;
        tx.commit().await.map_err(map_sqlx)?;
        Ok((
            self.current_recording(workspace_id, observation_id).await?,
            true,
        ))
    }

    /// What `record_wallet_purchase` would post right now, against the
    /// owner's current balance -- built from the same persisted evidence and
    /// the same posting primitive, but nothing is locked or written. A
    /// transaction that is already recorded has nothing to preview.
    pub async fn preview_wallet_purchase(
        &self,
        workspace_id: WorkspaceId,
        observation_id: Uuid,
    ) -> Result<InventoryPreview, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let target = load_recording_target(&mut tx, workspace_id, observation_id, false).await?;
        if ensure_recordable(&target)? {
            return Err(InventoryError::Validation(
                "This purchase is already recorded in inventory.".to_string(),
            ));
        }
        let mut posting = wallet_purchase_posting(&mut tx, workspace_id, &target.row).await?;
        let current = PgInventoryRepository::balance_with(&mut *tx, &posting.key)
            .await?
            .unwrap_or_else(|| {
                InventoryBalance::empty(posting.key.clone(), posting.type_name.clone())
            });
        tx.rollback().await.map_err(map_sqlx)?;
        // Recording posts at whatever revision it locks; preview against the
        // revision it sees now.
        posting.expected_revision = current.revision;
        let resulting = apply_inventory_event(&current, &posting)?;
        Ok(InventoryPreview {
            current,
            posting,
            resulting,
            warnings: Vec::new(),
        })
    }

    /// Reverts one recording of a wallet purchase: an exact compensating
    /// ledger event and the recording marked reverted (never deleted), so the
    /// transaction can be recorded again. Rejected
    /// as a whole if the ledger cannot absorb the reversal, e.g. because the
    /// stock has since been consumed.
    pub async fn revert_wallet_purchase_recording(
        &self,
        workspace_id: WorkspaceId,
        observation_id: Uuid,
        recording_id: Uuid,
    ) -> Result<FinanceInventoryRecording, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        // Same lock order as record and generic reversal: wallet transaction,
        // then recording, then balance.
        sqlx::query(
            r#"
            SELECT w.id
            FROM esi_wallet_transactions w
            JOIN eve_connections c ON c.id = w.connection_id
            WHERE w.id = $1 AND c.workspace_id = $2
            FOR UPDATE OF w
            "#,
        )
        .bind(observation_id)
        .bind(workspace_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?
        .ok_or(InventoryError::ItemNotFound)?;

        let recording = sqlx::query_as::<_, RecordingRow>(
            r#"
            SELECT s.owner_id, s.reverted_at, e.type_id
            FROM inventory_event_sources s
            JOIN inventory_events e ON e.id = s.inventory_event_id
            WHERE s.inventory_event_id = $1 AND s.observation_id = $2 AND s.workspace_id = $3
            FOR UPDATE OF s
            "#,
        )
        .bind(recording_id)
        .bind(observation_id)
        .bind(workspace_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?
        .ok_or(InventoryError::ItemNotFound)?;
        if recording.reverted_at.is_some() {
            return Err(InventoryError::AlreadyReversed);
        }

        let event_id = InventoryEventId(recording_id);
        lock_recorded_transaction(&mut tx, event_id).await?;
        let reversal_id = PgInventoryRepository::reverse_event_in_transaction(
            &mut tx,
            &InventoryItemKey {
workspace_id,
owner_id: OwnerId(recording.owner_id),
type_id: recording.type_id,
            },
            event_id,
            format!("Reverted inventory recording of EVE wallet transaction {observation_id}."),
        )
        .await
        .map_err(|error| match error {
            InventoryError::NegativeBalance | InventoryError::InvalidProjection => {
InventoryError::Validation(
    "This recording can't be reverted because some of the purchased stock has since been used or removed."
        .to_string(),
)
            }
            other => other,
        })?;
        settle_source_reversal(&mut tx, event_id, reversal_id).await?;
        tx.commit().await.map_err(map_sqlx)?;
        self.current_recording(workspace_id, observation_id).await
    }

    async fn current_recording(
        &self,
        workspace_id: WorkspaceId,
        observation_id: Uuid,
    ) -> Result<FinanceInventoryRecording, InventoryError> {
        recording_state(&self.pool, workspace_id, observation_id)
            .await?
            .ok_or(InventoryError::InvalidProjection)
    }
}

#[derive(sqlx::FromRow)]
struct RecordingRow {
    owner_id: Uuid,
    reverted_at: Option<DateTime<Utc>>,
    type_id: i64,
}

async fn load_recording_target(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    observation_id: Uuid,
    lock: bool,
) -> Result<RecordingTarget, InventoryError> {
    let sql = format!(
        r#"
        SELECT w.connection_id, c.owner_id,
               w.source_transaction_id, w.first_sync_run_id, w.type_id, w.quantity,
               w.unit_price, w.total_price, w.is_buy, w.is_personal, w.transacted_at
        FROM esi_wallet_transactions w
        JOIN eve_connections c ON c.id = w.connection_id
        JOIN owners o ON o.id = c.owner_id AND o.workspace_id = c.workspace_id
        WHERE w.id = $1
          AND c.workspace_id = $2
          AND c.disconnected_at IS NULL
        {}
        "#,
        if lock { "FOR UPDATE OF w" } else { "" }
    );
    let target = sqlx::query_as::<_, RecordingTargetRow>(&sql)
        .bind(observation_id)
        .bind(workspace_id.0)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map_sqlx)?
        .ok_or(InventoryError::ItemNotFound)?;
    // A separate statement on purpose: under READ COMMITTED it sees whatever a
    // racer committed while this one waited for the row lock above, which a
    // subquery in the locking statement would not.
    let recorded = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
          SELECT 1 FROM inventory_event_sources
          WHERE observation_id = $1
            AND accounting_effect_kind = 'purchase'
            AND reverted_at IS NULL
        )
        "#,
    )
    .bind(observation_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_sqlx)?;
    Ok(RecordingTarget {
        row: target,
        recorded,
    })
}

struct RecordingTarget {
    row: RecordingTargetRow,
    recorded: bool,
}

/// Rejects transactions that can never be recorded; otherwise returns whether
/// the transaction is already actively recorded.
fn ensure_recordable(target: &RecordingTarget) -> Result<bool, InventoryError> {
    if !target.row.is_buy {
        return Err(InventoryError::Validation(
            "Only Market Buy transactions can be added to inventory.".to_string(),
        ));
    }
    if !target.row.is_personal {
        return Err(InventoryError::Validation(
            "Only personal wallet purchases can be added to inventory.".to_string(),
        ));
    }
    Ok(target.recorded)
}

/// The purchase posting for one wallet transaction, built only from its
/// persisted evidence.
async fn wallet_purchase_posting(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    target: &RecordingTargetRow,
) -> Result<InventoryPosting, InventoryError> {
    let quantity = u64::try_from(target.quantity).map_err(|_| InventoryError::InvalidProjection)?;
    // The persisted total is the acquisition basis. It is `unit_price ×
    // quantity` by construction; refuse rather than guess if it isn't.
    let total =
        exact_total(target.unit_price, quantity).ok_or(InventoryError::ArithmeticOverflow)?;
    if total.0 != target.total_price {
        return Err(InventoryError::Validation(
            "The recorded transaction total does not match quantity × unit price.".to_string(),
        ));
    }
    let type_name = sqlx::query_scalar::<_, String>(
        r#"
        SELECT t.name_en
        FROM sde_types t
        JOIN sde_imports i ON i.id = t.import_id AND i.active
        WHERE t.type_id = $1
        "#,
    )
    .bind(target.type_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_sqlx)?
    .ok_or_else(|| {
        InventoryError::Validation("The EVE type is not in the active SDE.".to_string())
    })?;

    purchase_posting(
        workspace_id,
        OwnerId(target.owner_id),
        PostInventoryCommand {
            type_id: target.type_id,
            type_name,
            quantity,
            unit_cost: Some(target.unit_price.to_string()),
            cost_quality: CostInputQuality::Known,
            source_reference: format!("EVE wallet transaction {}", target.source_transaction_id),
            note: "Recorded from a Finance market buy.".to_string(),
            effective_at: target.transacted_at,
            expected_revision: 0,
            acknowledge_zero_cost: false,
        },
    )
}
