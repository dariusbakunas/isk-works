//! Explicit inventory recording against a ticket -- the domain shapes for
//! "Record acquisition" and "Record production".
//!
//! The governing rule: a ticket's workflow `status` never changes
//! inventory; inventory changes only through these explicit, named,
//! idempotent actions. A `RecordingState` is *derived* accounting/execution
//! state, orthogonal to `status` -- any `status` may pair with any
//! `RecordingState`.

use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TicketInventoryRecordingId(pub Uuid);

impl TicketInventoryRecordingId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TicketInventoryRecordingId {
    fn default() -> Self {
        Self::new()
    }
}

/// The kind of explicit inventory recording made against a ticket.
/// `Acquisition` posts one `Purchase`; `Production` posts N `Consumption`
/// events plus one `ProductionOutput`. Both share the
/// `ticket_inventory_recordings` table (see the field docs below for which
/// columns each kind uses).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TicketInventoryRecordingKind {
    Acquisition,
    Production,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TicketInventoryRecordingStatus {
    Recorded,
    Reversed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketInventoryEffect {
    pub event_id: InventoryEventId,
    pub kind: InventoryEventKind,
    pub type_id: i64,
    pub captured_name: String,
    pub quantity_delta: i64,
    pub total_cost_delta: MoneyDelta,
}

/// One immutable "I actually acquired / produced this" ledger row --
/// accounting provenance, never updated or deleted. Every inventory event
/// it posts carries `inventory_events.ticket_inventory_recording_id` back
/// to this row.
///
/// `recorded_quantity` is set only for `Acquisition`; `runs_completed` /
/// `installation_cost` / `output_type_id` / `output_quantity` only for
/// `Production` (the `ticket_inventory_recordings_*_shape` CHECKs enforce
/// this).
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketInventoryRecording {
    pub id: TicketInventoryRecordingId,
    pub ticket_id: TicketId,
    pub kind: TicketInventoryRecordingKind,
    /// Acquisition only. This recording's own acquired amount -- not
    /// cumulative, and never capped at the ticket's demand (surplus is
    /// legitimate).
    pub recorded_quantity: Option<u64>,
    /// Production only. Runs completed in this recording -- not cumulative,
    /// not capped at the planned run count.
    pub runs_completed: Option<u64>,
    /// Production only. The explicit actual installation cost attributed to
    /// this recording (never derived from the plan). `0` is valid.
    pub installation_cost: Option<Money>,
    /// Production only. The type this recording produced -- equals the
    /// ticket's product type.
    pub output_type_id: Option<i64>,
    /// Production only. Units produced in this recording. `0` is valid (a
    /// scrapped/failed job: inputs consumed, nothing produced).
    pub output_quantity: Option<u64>,
    /// Informational only -- inventory is not location-scoped.
    pub location_note: String,
    pub note: String,
    pub recorded_at: DateTime<Utc>,
    pub reverted_at: Option<DateTime<Utc>>,
    pub status: TicketInventoryRecordingStatus,
    pub effects: Vec<TicketInventoryEffect>,
}

/// Derived, never stored. The accounting counterpart to a ticket's
/// *workflow* `status`, and fully orthogonal to it: `status = Complete`
/// with `NotRecorded`, or `status = Todo` with `Recorded`, are both valid.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingState {
    NotRecorded,
    PartiallyRecorded,
    Recorded,
}

/// The derived recording rollup for one ticket, from its requested
/// quantity and the sum of its recordings' `recorded_quantity`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketRecordingSummary {
    pub state: RecordingState,
    pub requested_quantity: u64,
    pub recorded_quantity: u64,
    pub remaining_quantity: u64,
    pub surplus_quantity: u64,
}

/// Pure. For an Acquisition ticket `requested` is `ticket.quantity` and
/// `recorded` is `SUM(recorded_quantity)`. For a Manufacturing/Reaction
/// ticket `requested` is `execution_snapshot.runs` (falling back to
/// `recorded` when no plan was captured -- see the read model) and
/// `recorded` is `SUM(runs_completed)`. No I/O -- matches this module's
/// `derive_*` convention.
#[must_use]
pub fn derive_recording_summary(requested: u64, recorded: u64) -> TicketRecordingSummary {
    let remaining = requested.saturating_sub(recorded);
    let surplus = recorded.saturating_sub(requested);
    let state = if recorded == 0 {
        RecordingState::NotRecorded
    } else if recorded >= requested {
        RecordingState::Recorded
    } else {
        RecordingState::PartiallyRecorded
    };
    TicketRecordingSummary {
        state,
        requested_quantity: requested,
        recorded_quantity: recorded,
        remaining_quantity: remaining,
        surplus_quantity: surplus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording(reverted_at: Option<DateTime<Utc>>) -> TicketInventoryRecording {
        TicketInventoryRecording {
            id: TicketInventoryRecordingId::new(),
            ticket_id: TicketId::new(),
            kind: TicketInventoryRecordingKind::Acquisition,
            recorded_quantity: Some(100),
            runs_completed: None,
            installation_cost: None,
            output_type_id: None,
            output_quantity: None,
            location_note: "Jita 4-4".to_string(),
            note: "first trip".to_string(),
            recorded_at: Utc::now(),
            reverted_at,
            status: if reverted_at.is_some() {
                TicketInventoryRecordingStatus::Reversed
            } else {
                TicketInventoryRecordingStatus::Recorded
            },
            effects: Vec::new(),
        }
    }

    #[test]
    fn recording_status_is_derived_only_from_reverted_at() {
        assert_eq!(
            recording(None).status,
            TicketInventoryRecordingStatus::Recorded
        );
        assert_eq!(
            recording(Some(Utc::now())).status,
            TicketInventoryRecordingStatus::Reversed
        );
    }

    #[test]
    fn nothing_recorded_is_not_recorded() {
        let summary = derive_recording_summary(100_000, 0);
        assert_eq!(summary.state, RecordingState::NotRecorded);
        assert_eq!(summary.remaining_quantity, 100_000);
        assert_eq!(summary.surplus_quantity, 0);
    }

    #[test]
    fn some_but_not_all_is_partially_recorded() {
        let summary = derive_recording_summary(100_000, 75_000);
        assert_eq!(summary.state, RecordingState::PartiallyRecorded);
        assert_eq!(summary.recorded_quantity, 75_000);
        assert_eq!(summary.remaining_quantity, 25_000);
        assert_eq!(summary.surplus_quantity, 0);
    }

    #[test]
    fn exactly_the_requested_amount_is_recorded_with_no_surplus() {
        let summary = derive_recording_summary(100, 100);
        assert_eq!(summary.state, RecordingState::Recorded);
        assert_eq!(summary.remaining_quantity, 0);
        assert_eq!(summary.surplus_quantity, 0);
    }

    #[test]
    fn over_recording_is_recorded_with_surplus_and_zero_remaining() {
        let summary = derive_recording_summary(100_000, 105_000);
        assert_eq!(summary.state, RecordingState::Recorded);
        assert_eq!(summary.remaining_quantity, 0);
        assert_eq!(summary.surplus_quantity, 5_000);
    }

    #[test]
    fn recording_state_is_independent_of_the_requested_amount_being_tiny() {
        // A degenerate `requested` never underflows or panics.
        let summary = derive_recording_summary(1, 5);
        assert_eq!(summary.state, RecordingState::Recorded);
        assert_eq!(summary.surplus_quantity, 4);
        assert_eq!(summary.remaining_quantity, 0);
    }
}
