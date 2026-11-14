//! `RecordingStore` for Postgres: Ticket inventory recordings, their
//! previews and reversal.

use super::*;

use super::inventory_posting::*;
use super::recording_preview::*;
use super::recording_queries::*;

#[async_trait]
impl RecordingStore for PgOrderRepository {
    async fn available_quantity(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_id: i64,
    ) -> Result<u64, OrderError> {
        let balance: Option<i64> = sqlx::query_scalar(
            "SELECT quantity FROM inventory_balances WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?;
        let balance = u64_from_i64(balance.unwrap_or(0))?;

        let allocated: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(quantity), 0)::bigint FROM inventory_allocations \
             WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 \
             AND released_at IS NULL AND consumed_at IS NULL",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(type_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        let allocated = u64_from_i64(allocated)?;

        Ok(balance.saturating_sub(allocated))
    }

    async fn record_ticket_acquisition(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        input: RecordAcquisitionInput,
    ) -> Result<RecordAcquisitionOutcome, OrderError> {
        self.run_ticket_acquisition(workspace_id, ticket_id, input, None)
            .await
    }

    async fn preview_ticket_acquisition(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        mut input: RecordAcquisitionInput,
    ) -> Result<RecordingPreview, OrderError> {
        // A fresh key can never replay an existing recording.
        input.idempotency_key = Uuid::new_v4();
        let mut capture = PreviewCapture::default();
        self.run_ticket_acquisition(workspace_id, ticket_id, input, Some(&mut capture))
            .await?;
        Ok(capture.into_preview())
    }

    async fn record_ticket_production(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        input: RecordProductionInput,
    ) -> Result<RecordProductionOutcome, OrderError> {
        self.run_ticket_production(workspace_id, ticket_id, input, None)
            .await
    }

    async fn preview_ticket_production(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        mut input: RecordProductionInput,
    ) -> Result<RecordingPreview, OrderError> {
        // A fresh key can never replay an existing recording.
        input.idempotency_key = Uuid::new_v4();
        let mut capture = PreviewCapture::default();
        self.run_ticket_production(workspace_id, ticket_id, input, Some(&mut capture))
            .await?;
        Ok(capture.into_preview())
    }

    async fn revert_ticket_inventory_recording(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        recording_id: TicketInventoryRecordingId,
    ) -> Result<RevertTicketInventoryRecordingOutcome, OrderError> {
        #[derive(sqlx::FromRow)]
        struct LinkedEventRow {
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
            reversed_by_event_id: Option<Uuid>,
        }

        fn event_kind(value: &str) -> Result<InventoryEventKind, OrderError> {
            match value {
                "purchase" => Ok(InventoryEventKind::Purchase),
                "consumption" => Ok(InventoryEventKind::Consumption),
                "production_output" => Ok(InventoryEventKind::ProductionOutput),
                _ => Err(OrderError::RecordingEvidenceInvalid),
            }
        }

        fn cost_quality(value: &str) -> Result<CostInputQuality, OrderError> {
            match value {
                "known" => Ok(CostInputQuality::Known),
                "estimated" => Ok(CostInputQuality::Estimated),
                "zero_cost" => Ok(CostInputQuality::ZeroCost),
                _ => Err(OrderError::RecordingEvidenceInvalid),
            }
        }

        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let ticket: Option<(String, Uuid, Option<i64>, Option<serde_json::Value>)> =
            sqlx::query_as(
                "SELECT kind, owner_id, quantity, execution_snapshot FROM tickets \
                 WHERE workspace_id = $1 AND id = $2",
            )
            .bind(workspace_id.0)
            .bind(ticket_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
        let (ticket_kind, ticket_owner_id, requested_quantity, execution_snapshot) =
            ticket.ok_or(OrderError::TicketNotFound)?;

        let recording = sqlx::query_as::<_, TicketInventoryRecordingRow>(
            "SELECT id, ticket_id, kind, recorded_quantity, runs_completed, installation_cost, \
             output_type_id, output_quantity, location_note, note, recorded_at, reverted_at \
             FROM ticket_inventory_recordings WHERE id = $1 FOR UPDATE",
        )
        .bind(recording_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?
        .ok_or(OrderError::RecordingNotFound)?
        .into_recording()?;
        if recording.ticket_id != ticket_id {
            return Err(OrderError::RecordingNotFound);
        }
        if recording.reverted_at.is_some() {
            return Err(OrderError::RecordingAlreadyReversed);
        }

        let mut events = sqlx::query_as::<_, LinkedEventRow>(
            "SELECT e.id, e.workspace_id, e.owner_id, e.type_id, e.captured_name, \
                    e.event_kind, e.quantity_delta, e.total_cost_delta, e.unit_cost, \
                    e.cost_quality, e.source_reference, reversed.id AS reversed_by_event_id \
             FROM inventory_events e \
             LEFT JOIN inventory_events reversed ON reversed.reverses_event_id = e.id \
             WHERE e.ticket_inventory_recording_id = $1 AND e.event_kind <> 'reversal'",
        )
        .bind(recording_id.0)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?;

        if events.iter().any(|event| {
            event.workspace_id != workspace_id.0
                || event.owner_id != ticket_owner_id
                || event.reversed_by_event_id.is_some()
        }) {
            return Err(OrderError::RecordingEvidenceInvalid);
        }
        match recording.kind {
            TicketInventoryRecordingKind::Acquisition => {
                let expected_quantity = recording
                    .recorded_quantity
                    .and_then(|quantity| i64::try_from(quantity).ok());
                if events.len() != 1
                    || events[0].event_kind != "purchase"
                    || Some(events[0].quantity_delta) != expected_quantity
                    || events[0].quantity_delta <= 0
                    || events[0].total_cost_delta.is_sign_negative()
                {
                    return Err(OrderError::RecordingEvidenceInvalid);
                }
            }
            TicketInventoryRecordingKind::Production => {
                let expected_output_quantity = recording.output_quantity.unwrap_or(0);
                let outputs: Vec<&LinkedEventRow> = events
                    .iter()
                    .filter(|event| event.event_kind == "production_output")
                    .collect();
                let consumptions: Vec<&LinkedEventRow> = events
                    .iter()
                    .filter(|event| event.event_kind == "consumption")
                    .collect();
                let expected_output_count = usize::from(expected_output_quantity > 0);
                let invalid_shape = outputs.len() != expected_output_count
                    || events.len() != outputs.len() + consumptions.len()
                    || consumptions.iter().any(|event| {
                        event.quantity_delta >= 0 || event.total_cost_delta > Decimal::ZERO
                    });
                let invalid_output = outputs.first().is_some_and(|output| {
                    output.type_id != recording.output_type_id.unwrap_or_default()
                        || output.quantity_delta
                            != i64::try_from(expected_output_quantity).unwrap_or(i64::MIN)
                        || output.quantity_delta <= 0
                        || output.total_cost_delta.is_sign_negative()
                });
                let consumed_basis = consumptions
                    .iter()
                    .fold(Decimal::ZERO, |total, event| total - event.total_cost_delta);
                let mut expected_basis = consumed_basis
                    + recording
                        .installation_cost
                        .map(|money| money.0)
                        .unwrap_or(Decimal::ZERO);
                expected_basis.rescale(4);
                let invalid_basis = match outputs.first() {
                    Some(output) => output.total_cost_delta != expected_basis,
                    None => !expected_output_quantity.eq(&0),
                };
                if invalid_shape || invalid_output || invalid_basis {
                    return Err(OrderError::RecordingEvidenceInvalid);
                }
            }
        }

        let mut keys: Vec<(Uuid, Uuid, i64)> = events
            .iter()
            .map(|event| (event.workspace_id, event.owner_id, event.type_id))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        let mut revisions = HashMap::with_capacity(keys.len());
        for (event_workspace_id, event_owner_id, type_id) in keys {
            let revision: Option<i64> = sqlx::query_scalar(
                "SELECT revision FROM inventory_balances \
                 WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 FOR UPDATE",
            )
            .bind(event_workspace_id)
            .bind(event_owner_id)
            .bind(type_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
            revisions.insert(
                (event_workspace_id, event_owner_id, type_id),
                u64_from_i64(revision.ok_or(OrderError::RecordingEvidenceInvalid)?)?,
            );
        }

        // Balance locks serialize every writer affecting these identities.
        // Re-read reversal state only now so a generic reversal that raced
        // before the locks cannot be missed by the earlier evidence load.
        let reversed_while_waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inventory_events reversal \
             JOIN inventory_events original ON original.id = reversal.reverses_event_id \
             WHERE original.ticket_inventory_recording_id = $1)",
        )
        .bind(recording_id.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_error)?;
        if reversed_while_waiting {
            return Err(OrderError::RecordingAlreadyReversed);
        }

        events.sort_by_key(|event| {
            let compensation = event.quantity_delta.saturating_neg();
            (
                compensation >= 0,
                event.workspace_id,
                event.owner_id,
                event.type_id,
                event.id,
            )
        });
        let now = crate::db_now();
        for event in events {
            let key_tuple = (event.workspace_id, event.owner_id, event.type_id);
            let expected_revision = *revisions
                .get(&key_tuple)
                .ok_or(OrderError::RecordingEvidenceInvalid)?;
            let posting = InventoryPosting {
                id: InventoryEventId::new(),
                key: InventoryItemKey {
                    workspace_id: WorkspaceId(event.workspace_id),
                    owner_id: OwnerId(event.owner_id),
                    type_id: event.type_id,
                },
                type_name: event.captured_name,
                kind: InventoryEventKind::Reversal,
                quantity_delta: event
                    .quantity_delta
                    .checked_neg()
                    .ok_or(OrderError::RecordingEvidenceInvalid)?,
                total_cost_delta: MoneyDelta(if event.total_cost_delta.is_zero() {
                    Decimal::ZERO
                } else {
                    -event.total_cost_delta
                }),
                unit_cost: event.unit_cost.map(Money),
                cost_quality: cost_quality(&event.cost_quality)?,
                source_reference: event.source_reference,
                note: "Ticket recording reverted".to_string(),
                effective_at: now,
                recorded_at: now,
                expected_revision,
                reverses_event_id: Some(InventoryEventId(event.id)),
            };
            event_kind(&event.event_kind)?;
            let reversal_id = posting.id;
            let resulting =
                PgInventoryRepository::post_historical_reversal_in_transaction(&mut tx, &posting)
                    .await
                    .map_err(|error| match error {
                        iskworks_core::InventoryError::AlreadyReversed => {
                            OrderError::RecordingAlreadyReversed
                        }
                        _ => OrderError::RecordingReversalInvalid,
                    })?;
            sqlx::query(
                "UPDATE inventory_events SET ticket_inventory_recording_id = $1 WHERE id = $2",
            )
            .bind(recording_id.0)
            .bind(reversal_id.0)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                if is_unique_violation(&error) {
                    OrderError::RecordingAlreadyReversed
                } else {
                    map_error(error)
                }
            })?;
            revisions.insert(key_tuple, resulting.revision);
        }

        let reverted_at = crate::db_now();
        let updated = sqlx::query(
            "UPDATE ticket_inventory_recordings SET reverted_at = $1 \
             WHERE id = $2 AND reverted_at IS NULL",
        )
        .bind(reverted_at)
        .bind(recording_id.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if updated.rows_affected() != 1 {
            return Err(OrderError::RecordingAlreadyReversed);
        }
        super::reservations::undo_recording_allocations(&mut tx, recording_id, reverted_at).await?;

        let recorded = match recording.kind {
            TicketInventoryRecordingKind::Acquisition => {
                recorded_quantity_sum(&mut tx, ticket_id).await?
            }
            TicketInventoryRecordingKind::Production => {
                recorded_runs_sum(&mut tx, ticket_id).await?
            }
        };
        let requested = match ticket_kind_from_str(&ticket_kind)? {
            TicketKind::Acquisition => requested_quantity
                .map(u64_from_i64)
                .transpose()?
                .unwrap_or_default(),
            TicketKind::Manufacturing | TicketKind::Reaction => {
                execution_snapshot_runs(&execution_snapshot)?.unwrap_or(recorded)
            }
            TicketKind::Generic => recorded,
        };
        tx.commit().await.map_err(map_error)?;

        Ok(RevertTicketInventoryRecordingOutcome {
            recording: TicketInventoryRecording {
                reverted_at: Some(reverted_at),
                status: TicketInventoryRecordingStatus::Reversed,
                ..recording
            },
            summary: derive_recording_summary(requested, recorded),
        })
    }

    async fn list_ticket_inventory_recordings(
        &self,
        ticket_id: TicketId,
    ) -> Result<Vec<TicketInventoryRecording>, OrderError> {
        self.list_ticket_inventory_recordings_for_tickets(&[ticket_id])
            .await
    }

    async fn list_ticket_inventory_recordings_for_tickets(
        &self,
        ticket_ids: &[TicketId],
    ) -> Result<Vec<TicketInventoryRecording>, OrderError> {
        let mut recordings = sqlx::query_as::<_, TicketInventoryRecordingRow>(
            "SELECT id, ticket_id, kind, recorded_quantity, runs_completed, \
             installation_cost, output_type_id, output_quantity, location_note, note, recorded_at, reverted_at \
             FROM ticket_inventory_recordings WHERE ticket_id = ANY($1) \
             ORDER BY ticket_id, recorded_at ASC, id ASC",
        )
        .bind(ticket_ids.iter().map(|id| id.0).collect::<Vec<_>>())
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(TicketInventoryRecordingRow::into_recording)
        .collect::<Result<Vec<_>, _>>()?;
        if recordings.is_empty() {
            return Ok(recordings);
        }
        let rows: Vec<(Uuid, Uuid, String, i64, String, i64, Decimal)> = sqlx::query_as(
            "SELECT id, ticket_inventory_recording_id, event_kind, type_id, captured_name, \
             quantity_delta, total_cost_delta \
             FROM inventory_events WHERE ticket_inventory_recording_id = ANY($1) \
             AND event_kind <> 'reversal' ORDER BY sequence, id",
        )
        .bind(
            recordings
                .iter()
                .map(|recording| recording.id.0)
                .collect::<Vec<_>>(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        let mut effects_by_recording: HashMap<Uuid, Vec<TicketInventoryEffect>> = HashMap::new();
        for (id, recording_id, kind, type_id, captured_name, quantity_delta, total_cost_delta) in
            rows
        {
            let kind = match kind.as_str() {
                "purchase" => InventoryEventKind::Purchase,
                "consumption" => InventoryEventKind::Consumption,
                "production_output" => InventoryEventKind::ProductionOutput,
                _ => return Err(OrderError::RecordingEvidenceInvalid),
            };
            effects_by_recording
                .entry(recording_id)
                .or_default()
                .push(TicketInventoryEffect {
                    event_id: InventoryEventId(id),
                    kind,
                    type_id,
                    captured_name,
                    quantity_delta,
                    total_cost_delta: MoneyDelta(total_cost_delta),
                });
        }
        for recording in &mut recordings {
            recording.effects = effects_by_recording
                .remove(&recording.id.0)
                .unwrap_or_default();
        }
        Ok(recordings)
    }
}

impl PgOrderRepository {
    /// Belt-and-braces path for `record_ticket_acquisition` /
    /// `record_ticket_production` when the recording INSERT hit the
    /// `(ticket_id, idempotency_key)` UNIQUE (a concurrent duplicate that
    /// somehow bypassed the ticket row lock). The winning transaction has
    /// committed; read its recording back on a fresh connection and return
    /// it, plus the kind-appropriate recorded total, as an idempotent
    /// replay -- the caller wraps it in the right outcome type.
    async fn replay_ticket_recording(
        &self,
        ticket_id: TicketId,
        idempotency_key: uuid::Uuid,
    ) -> Result<(TicketInventoryRecording, u64), OrderError> {
        let recording = sqlx::query_as::<_, TicketInventoryRecordingRow>(
            "SELECT id, ticket_id, kind, recorded_quantity, runs_completed, \
             installation_cost, output_type_id, output_quantity, location_note, note, recorded_at, reverted_at \
             FROM ticket_inventory_recordings WHERE ticket_id = $1 AND idempotency_key = $2",
        )
        .bind(ticket_id.0)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?
        .ok_or_else(|| {
            OrderError::Persistence(
                "recording disappeared after a unique-violation replay".to_string(),
            )
        })?
        .into_recording()?;
        let column = match recording.kind {
            TicketInventoryRecordingKind::Acquisition => "recorded_quantity",
            TicketInventoryRecordingKind::Production => "runs_completed",
        };
        let sum: i64 = sqlx::query_scalar(&format!(
            "SELECT COALESCE(SUM({column}), 0)::bigint \
             FROM ticket_inventory_recordings WHERE ticket_id = $1 AND reverted_at IS NULL"
        ))
        .bind(ticket_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        Ok((recording, u64_from_i64(sum)?))
    }
}

/// `kind, owner_id, type_id, captured_name, quantity, display_id,
/// acquisition_run_id, estimated_unit_cost` of the ticket being recorded.
type AcquisitionTicketRow = (
    String,
    Uuid,
    i64,
    String,
    i64,
    String,
    Option<Uuid>,
    Option<Decimal>,
);

impl PgOrderRepository {
    /// The body of `record_ticket_acquisition`. With `preview`, it records into the
    /// transaction, captures the touched balances before and after, then
    /// rolls back instead of committing.
    pub(super) async fn run_ticket_acquisition(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        input: RecordAcquisitionInput,
        mut preview: Option<&mut PreviewCapture>,
    ) -> Result<RecordAcquisitionOutcome, OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;

        // Lock the ticket row -- serializes concurrent recordings for this
        // same ticket so the idempotency pre-check below is race-free. The
        // `UNIQUE (ticket_id, idempotency_key)` constraint is the
        // DB-authoritative backstop regardless (handled after the INSERT).
        let ticket_row: Option<AcquisitionTicketRow> = sqlx::query_as(
            "SELECT kind, owner_id, type_id, captured_name, quantity, display_id, \
                 acquisition_run_id, estimated_unit_cost FROM tickets \
                 WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(workspace_id.0)
        .bind(ticket_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        let (
            kind_str,
            owner_id,
            type_id,
            captured_name,
            requested_quantity,
            display_id,
            acquisition_run_id,
            estimated_unit_cost,
        ) = ticket_row.ok_or(OrderError::TicketNotFound)?;

        if ticket_kind_from_str(&kind_str)? != TicketKind::Acquisition {
            return Err(OrderError::RecordingRequiresAcquisitionTicket);
        }
        if acquisition_run_id.is_some() {
            return Err(OrderError::RecordingNotAllowedForBatchedTicket);
        }
        let owner_id = OwnerId(owner_id);
        let requested_quantity = u64_from_i64(requested_quantity)?;
        let estimated_unit_cost = estimated_unit_cost.map(Money);

        // Idempotent replay: this key was already recorded -- post nothing.
        if let Some(existing) =
            find_ticket_recording_by_key(&mut tx, ticket_id, input.idempotency_key).await?
        {
            let recorded = recorded_quantity_sum(&mut tx, ticket_id).await?;
            tx.commit().await.map_err(map_error)?;
            return Ok(RecordAcquisitionOutcome {
                recording: existing,
                created: false,
                summary: derive_recording_summary(requested_quantity, recorded),
            });
        }

        if let Some(capture) = preview.as_deref_mut() {
            capture
                .read_before(
                    &mut tx,
                    vec![(
                        InventoryItemKey {
                            workspace_id,
                            owner_id,
                            type_id,
                        },
                        captured_name.clone(),
                        RecordingPreviewEffect::Added,
                    )],
                )
                .await?;
        }

        let recording_id = TicketInventoryRecordingId::new();
        let recorded_at = crate::db_now();
        let insert = sqlx::query(
            "INSERT INTO ticket_inventory_recordings \
               (id, ticket_id, kind, idempotency_key, recorded_quantity, \
                location_note, note, recorded_at) \
             VALUES ($1, $2, 'acquisition', $3, $4, $5, $6, $7)",
        )
        .bind(recording_id.0)
        .bind(ticket_id.0)
        .bind(input.idempotency_key)
        .bind(i64_from_u64(input.quantity)?)
        .bind(&input.location_note)
        .bind(&input.note)
        .bind(recorded_at)
        .execute(&mut *tx)
        .await;
        if let Err(error) = insert {
            if is_unique_violation(&error) {
                // A concurrent duplicate won the race (not reachable under
                // the ticket `FOR UPDATE`, but the UNIQUE is authoritative).
                tx.rollback().await.map_err(map_error)?;
                let (recording, recorded) = self
                    .replay_ticket_recording(ticket_id, input.idempotency_key)
                    .await?;
                return Ok(RecordAcquisitionOutcome {
                    recording,
                    created: false,
                    summary: derive_recording_summary(requested_quantity, recorded),
                });
            }
            return Err(map_error(error));
        }

        // One `Purchase` for exactly this recording's quantity, cost via the
        // same hierarchy `complete_ticket`'s acquisition path uses. Keeps
        // `display_id` as `source_reference` (existing convention); the
        // recording link below is the stronger provenance.
        let event_id = post_purchase_in_transaction(
            &mut tx,
            InventoryPostingLine {
                key: InventoryItemKey {
                    workspace_id,
                    owner_id,
                    type_id,
                },
                type_name: &captured_name,
                quantity: input.quantity,
                source_reference: &display_id,
            },
            input.unit_cost,
            estimated_unit_cost,
            input.effective_at,
            recorded_at,
        )
        .await?;

        sqlx::query("UPDATE inventory_events SET ticket_inventory_recording_id = $1 WHERE id = $2")
            .bind(recording_id.0)
            .bind(event_id.0)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;

        // The purchase is reserved to the Epic requirements this ticket
        // fulfills, up to what they still need; the rest stays free.
        super::reservations::reserve_recorded_output(
            &mut tx,
            super::reservations::RecordedOutput {
                workspace_id,
                owner_id,
                feeds: super::reservations::OutputFeeds::Acquisition { ticket_id },
                type_id,
                quantity: input.quantity,
                recording_id,
                now: recorded_at,
            },
        )
        .await?;

        let recorded = recorded_quantity_sum(&mut tx, ticket_id).await?;
        match preview {
            Some(capture) => {
                capture.read_after(&mut tx).await?;
                tx.rollback().await.map_err(map_error)?;
            }
            None => tx.commit().await.map_err(map_error)?,
        }

        Ok(RecordAcquisitionOutcome {
            recording: TicketInventoryRecording {
                id: recording_id,
                ticket_id,
                kind: TicketInventoryRecordingKind::Acquisition,
                recorded_quantity: Some(input.quantity),
                runs_completed: None,
                installation_cost: None,
                output_type_id: None,
                output_quantity: None,
                location_note: input.location_note,
                note: input.note,
                recorded_at,
                reverted_at: None,
                status: TicketInventoryRecordingStatus::Recorded,
                effects: Vec::new(),
            },
            created: true,
            summary: derive_recording_summary(requested_quantity, recorded),
        })
    }

    /// The body of `record_ticket_production`. With `preview`, it records into the
    /// transaction, captures the touched balances before and after, then
    /// rolls back instead of committing.
    pub(super) async fn run_ticket_production(
        &self,
        workspace_id: WorkspaceId,
        ticket_id: TicketId,
        input: RecordProductionInput,
        mut preview: Option<&mut PreviewCapture>,
    ) -> Result<RecordProductionOutcome, OrderError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;

        // Lock the ticket -- serializes concurrent recordings for it, so
        // the idempotency pre-check is race-free (the UNIQUE is the
        // authoritative backstop after the INSERT).
        let ticket_row: Option<(String, Uuid, i64, String, String, Option<serde_json::Value>)> =
            sqlx::query_as(
                "SELECT kind, owner_id, type_id, captured_name, display_id, execution_snapshot \
                 FROM tickets WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
            )
            .bind(workspace_id.0)
            .bind(ticket_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
        let (kind_str, owner_id, ticket_type_id, ticket_captured_name, display_id, snapshot_json) =
            ticket_row.ok_or(OrderError::TicketNotFound)?;

        let kind = ticket_kind_from_str(&kind_str)?;
        if !matches!(kind, TicketKind::Manufacturing | TicketKind::Reaction) {
            return Err(OrderError::RecordingRequiresProductionTicket);
        }
        let owner_id = OwnerId(owner_id);
        let requested_runs = execution_snapshot_runs(&snapshot_json)?;

        // The output must be the ticket's product.
        if input.output_type_id != ticket_type_id {
            return Err(OrderError::RecordingOutputTypeMismatch);
        }

        // Every input must be one of the ticket's frozen prerequisites (its
        // material list). Quantities are actuals -- not checked against the
        // plan. NOTE (known limitation): a legitimate unplanned consumable
        // not in the frozen BOM is rejected here, rather than warned about.
        let prereq_names: HashMap<i64, String> = sqlx::query_as::<_, (i64, String)>(
            "SELECT type_id, captured_name FROM ticket_prerequisites WHERE ticket_id = $1",
        )
        .bind(ticket_id.0)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?
        .into_iter()
        .collect();
        for line in &input.inputs {
            if line.quantity == 0 {
                return Err(OrderError::InvalidQuantity);
            }
            if !prereq_names.contains_key(&line.type_id) {
                return Err(OrderError::RecordingInputNotAPrerequisite);
            }
        }

        // Idempotent replay: this key was already recorded -- post nothing.
        if let Some(existing) =
            find_ticket_recording_by_key(&mut tx, ticket_id, input.idempotency_key).await?
        {
            let recorded = recorded_runs_sum(&mut tx, ticket_id).await?;
            if preview.is_none() {
                super::epic_completion::complete_epic_if_finished(
                    &mut tx,
                    workspace_id,
                    ticket_id,
                    crate::db_now(),
                )
                .await?;
            }
            tx.commit().await.map_err(map_error)?;
            return Ok(RecordProductionOutcome {
                recording: existing,
                created: false,
                summary: derive_recording_summary(requested_runs.unwrap_or(recorded), recorded),
            });
        }

        if let Some(capture) = preview.as_deref_mut() {
            let key = |type_id| InventoryItemKey {
                workspace_id,
                owner_id,
                type_id,
            };
            let mut items = vec![(
                key(input.output_type_id),
                ticket_captured_name.clone(),
                RecordingPreviewEffect::Added,
            )];
            for line in &input.inputs {
                if items
                    .iter()
                    .all(|(item, _, _)| item.type_id != line.type_id)
                {
                    items.push((
                        key(line.type_id),
                        prereq_names[&line.type_id].clone(),
                        RecordingPreviewEffect::Consumed,
                    ));
                }
            }
            capture.read_before(&mut tx, items).await?;
        }

        let recording_id = TicketInventoryRecordingId::new();
        let recorded_at = crate::db_now();
        let insert = sqlx::query(
            "INSERT INTO ticket_inventory_recordings \
               (id, ticket_id, kind, idempotency_key, runs_completed, installation_cost, \
                output_type_id, output_quantity, location_note, note, recorded_at) \
             VALUES ($1, $2, 'production', $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(recording_id.0)
        .bind(ticket_id.0)
        .bind(input.idempotency_key)
        .bind(i64_from_u64(input.runs_completed)?)
        .bind(input.installation_cost.0)
        .bind(input.output_type_id)
        .bind(i64_from_u64(input.output_quantity)?)
        .bind(&input.location_note)
        .bind(&input.note)
        .bind(recorded_at)
        .execute(&mut *tx)
        .await;
        if let Err(error) = insert {
            if is_unique_violation(&error) {
                tx.rollback().await.map_err(map_error)?;
                let (recording, recorded) = self
                    .replay_ticket_recording(ticket_id, input.idempotency_key)
                    .await?;
                return Ok(RecordProductionOutcome {
                    recording,
                    created: false,
                    summary: derive_recording_summary(requested_runs.unwrap_or(recorded), recorded),
                });
            }
            return Err(map_error(error));
        }

        // Every multi-balance inventory writer takes the complete lock set
        // in the same deterministic order as recording reversal. The
        // posting helpers below re-read already-held rows for their exact
        // revision and basis, but cannot deadlock by acquiring identities
        // in caller-provided input order.
        let mut balance_type_ids: Vec<i64> = input
            .inputs
            .iter()
            .map(|line| line.type_id)
            .chain((input.output_quantity > 0).then_some(input.output_type_id))
            .collect();
        balance_type_ids.sort_unstable();
        balance_type_ids.dedup();
        for type_id in balance_type_ids {
            sqlx::query(
                "SELECT revision FROM inventory_balances \
                 WHERE workspace_id = $1 AND owner_id = $2 AND type_id = $3 FOR UPDATE",
            )
            .bind(workspace_id.0)
            .bind(owner_id.0)
            .bind(type_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_error)?;
        }

        // Decide where each input type draws from, under those locks: the
        // ticket's own Epic reservations first, then free stock, then only
        // Epics the caller allowed. Refuse (rolling back the recording) if
        // that would use stock another Epic is counting on. A combined
        // ticket's own reservations span every Epic it serves.
        let consumers = super::reservations::ticket_operations(&mut *tx, &[ticket_id.0])
            .await?
            .remove(&ticket_id)
            .unwrap_or_default();
        let mut input_quantities: BTreeMap<i64, u64> = BTreeMap::new();
        for line in &input.inputs {
            *input_quantities.entry(line.type_id).or_insert(0) += line.quantity;
        }
        let draws = super::reservations::plan_recording_draws(
            &mut tx,
            workspace_id,
            owner_id,
            &consumers,
            &input_quantities,
            &input.take_from,
        )
        .await?;
        super::reservations::apply_recording_draws(&mut tx, &draws, recording_id, recorded_at)
            .await?;

        // Consume each actual input at the balance's current weighted
        // average; sum the real ledger basis removed.
        let mut consumed_basis = Decimal::ZERO;
        let mut event_ids: Vec<Uuid> = Vec::with_capacity(input.inputs.len() + 1);
        for line in &input.inputs {
            let type_name = prereq_names
                .get(&line.type_id)
                .expect("validated to be a prerequisite above");
            let (event_id, consumed) = post_consumption_in_transaction(
                &mut tx,
                InventoryPostingLine {
                    key: InventoryItemKey {
                        workspace_id,
                        owner_id,
                        type_id: line.type_id,
                    },
                    type_name,
                    quantity: line.quantity,
                    source_reference: &display_id,
                },
                recorded_at,
            )
            .await?;
            consumed_basis += consumed;
            event_ids.push(event_id.0);
        }

        // Production batch basis = actual consumed material basis + actual
        // installation cost. Assigned to the output in full -- every
        // produced unit (planned and surplus alike) shares it per-unit.
        let mut production_basis = consumed_basis + input.installation_cost.0;
        production_basis.rescale(4);

        if input.output_quantity > 0 {
            // Display unit cost only; `total_cost_delta` stays authoritative
            // (`apply_inventory_event` re-derives the average from it).
            let unit_cost = Money(production_basis)
                .checked_div_quantity(input.output_quantity)
                .map_err(|error| OrderError::Persistence(error.to_string()))?;
            let output_event = post_production_output_in_transaction(
                &mut tx,
                InventoryPostingLine {
                    key: InventoryItemKey {
                        workspace_id,
                        owner_id,
                        type_id: input.output_type_id,
                    },
                    type_name: &ticket_captured_name,
                    quantity: input.output_quantity,
                    source_reference: &display_id,
                },
                MoneyDelta(production_basis),
                Some(unit_cost),
                // Known: consumed basis is from the actual ledger and
                // installation cost is explicit -- nothing estimated.
                CostInputQuality::Known,
                input.effective_at,
                recorded_at,
            )
            .await?;
            event_ids.push(output_event.0);

            // The output is reserved to the steps this ticket's operations
            // feed, up to what they still need; overshoot stays free.
            if !consumers.is_empty() {
                super::reservations::reserve_recorded_output(
                    &mut tx,
                    super::reservations::RecordedOutput {
                        workspace_id,
                        owner_id,
                        feeds: super::reservations::OutputFeeds::Production {
                            operations: &consumers,
                        },
                        type_id: input.output_type_id,
                        quantity: input.output_quantity,
                        recording_id,
                        now: recorded_at,
                    },
                )
                .await?;
            }
        }

        sqlx::query(
            "UPDATE inventory_events SET ticket_inventory_recording_id = $1 WHERE id = ANY($2)",
        )
        .bind(recording_id.0)
        .bind(&event_ids)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        let recorded = recorded_runs_sum(&mut tx, ticket_id).await?;
        match preview {
            Some(capture) => {
                capture.read_after(&mut tx).await?;
                tx.rollback().await.map_err(map_error)?;
            }
            None => {
                super::epic_completion::complete_epic_if_finished(
                    &mut tx,
                    workspace_id,
                    ticket_id,
                    crate::db_now(),
                )
                .await?;
                tx.commit().await.map_err(map_error)?;
            }
        }

        Ok(RecordProductionOutcome {
            recording: TicketInventoryRecording {
                id: recording_id,
                ticket_id,
                kind: TicketInventoryRecordingKind::Production,
                recorded_quantity: None,
                runs_completed: Some(input.runs_completed),
                installation_cost: Some(input.installation_cost),
                output_type_id: Some(input.output_type_id),
                output_quantity: Some(input.output_quantity),
                location_note: input.location_note,
                note: input.note,
                recorded_at,
                reverted_at: None,
                status: TicketInventoryRecordingStatus::Recorded,
                effects: Vec::new(),
            },
            created: true,
            summary: derive_recording_summary(requested_runs.unwrap_or(recorded), recorded),
        })
    }
}
