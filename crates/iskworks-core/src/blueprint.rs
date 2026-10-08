use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{BuildId, OwnerId, WorkspaceId};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlueprintSourceMode {
    Manual,
    ObservedAsset,
    LegacyMigration,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BlueprintKind {
    Original,
    Copy,
    /// Doubles as the `BlueprintSelection::ObservedAsset` "effective config
    /// not yet captured" sentinel (see that variant's doc comment) -- a real
    /// observation always resolves to `Original` or `Copy`, so this value on
    /// an `ObservedAsset` selection unambiguously means "resolve/backfill
    /// this before trusting it for planning math."
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintObservation {
    pub id: Uuid,
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    pub owner_name: String,
    pub eve_item_id: i64,
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub kind: BlueprintKind,
    pub material_efficiency: u8,
    pub time_efficiency: u8,
    pub licensed_runs: Option<u64>,
    pub location_id: i64,
    pub location_flag: String,
    pub location_name: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub imported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintSnapshot {
    pub id: Uuid,
    pub build_id: BuildId,
    pub source_mode: BlueprintSourceMode,
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub kind: BlueprintKind,
    pub material_efficiency: u8,
    pub time_efficiency: u8,
    pub licensed_runs: Option<u64>,
    pub requested_runs: u64,
    pub source_observation_id: Option<Uuid>,
    pub source_eve_item_id: Option<i64>,
    pub source_owner_id: Option<OwnerId>,
    pub source_owner_name: Option<String>,
    pub source_location_id: Option<i64>,
    pub source_location_name: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub imported_at: Option<DateTime<Utc>>,
    pub manual_notes: Option<String>,
    pub planned_duration_seconds: Option<u64>,
    pub formula_version: String,
    pub captured_at: DateTime<Utc>,
}

impl BlueprintSnapshot {
    /// The most runs one industry job of this blueprint may run: a copy's
    /// licensed runs. `None` (no per-job limit) for an Original, an unknown
    /// kind, or a copy whose licensed runs were never captured -- planned as
    /// one job, as before multi-BPC job splits existed.
    #[must_use]
    pub fn max_runs_per_job(&self) -> Option<u64> {
        max_runs_per_job(self.kind, self.licensed_runs)
    }
}

/// See [`BlueprintSnapshot::max_runs_per_job`].
#[must_use]
pub fn max_runs_per_job(kind: BlueprintKind, licensed_runs: Option<u64>) -> Option<u64> {
    match kind {
        BlueprintKind::Copy => licensed_runs.filter(|runs| *runs > 0),
        BlueprintKind::Original | BlueprintKind::Unknown => None,
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BlueprintSelection {
    Manual {
        kind: BlueprintKind,
        material_efficiency: u8,
        time_efficiency: u8,
        licensed_runs: Option<u64>,
        #[serde(default)]
        notes: String,
    },
    /// `observation_id` is **provenance**: "this physical blueprint asset
    /// supplied the configuration below." It is not, and must never become,
    /// "this asset must keep existing for planning to work." `kind`,
    /// `material_efficiency` and `time_efficiency` are the durable
    /// **effective planning configuration**, captured once from the
    /// observation at selection time (see
    /// `IndustryService::capture_effective_blueprint_selection`) and frozen
    /// from then on -- ordinary planning (preview, materials/cost
    /// projection, Worksheet, Graph, candidate-preview, Epic freeze) reads
    /// these fields directly and must never re-resolve `observation_id`
    /// merely to rediscover them. A later change to the physical asset (BPC
    /// consumed, moved, ME/TE re-run) never silently rewrites an already
    /// -captured selection; the user must explicitly re-select a blueprint
    /// for that. Whether the named asset is *currently* observable/
    /// sufficient is execution-readiness evidence, checked separately, never
    /// a planning-validity gate.
    ///
    /// `kind == BlueprintKind::Unknown` is the sentinel for "effective
    /// config not yet captured" -- either a fresh selection that named only
    /// `observation_id` (client sends this; the server resolves and fills
    /// in the rest before persisting) or a legacy row persisted before
    /// these fields existed (backfilled by
    /// `IndustryService::backfill_observed_blueprint_configurations`, or
    /// left as-is, never fabricated, if its observation is unresolvable).
    ObservedAsset {
        observation_id: Uuid,
        #[serde(default)]
        kind: BlueprintKind,
        #[serde(default)]
        material_efficiency: u8,
        #[serde(default)]
        time_efficiency: u8,
        /// The observed copy's licensed runs, frozen with kind/ME/TE: the
        /// per-job run limit for multi-BPC job splits. `None` for an
        /// Original, or a selection captured before this field existed
        /// (planned as one job until backfilled or re-selected). Omitted
        /// from JSON when `None`, so earlier selections serialize unchanged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        licensed_runs: Option<u64>,
    },
}

impl BlueprintSelection {
    /// `true` once an `ObservedAsset` selection has captured its effective
    /// kind/ME/TE -- `false` for the "not yet captured" sentinel (a fresh
    /// client selection naming only `observation_id`, or an unmigrated
    /// legacy row). Always `true` for `Manual`, which never needed live
    /// resolution in the first place.
    #[must_use]
    pub fn is_effective_config_captured(&self) -> bool {
        match self {
            BlueprintSelection::Manual { .. } => true,
            BlueprintSelection::ObservedAsset { kind, .. } => *kind != BlueprintKind::Unknown,
        }
    }
}

/// Whether the blueprint a manufacturing ticket's `TaskExecutionSnapshot`
/// captured is actually usable *right now* -- distinct from whether the
/// underlying Build has a blueprint selection at all (that's `NotAssigned`
/// vs `Some(snapshot)`). Only `ObservedAsset` snapshots can be checked
/// against real, current state; a `Manual` snapshot is an assumption with
/// no owned asset behind it, so no availability claim is made for it (see
/// the "no fabricated blueprint availability" constraint in the design
/// doc) -- it reports `Assumed`, never `Suitable` or a shortage.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BlueprintAvailability {
    NotAssigned,
    Assumed,
    Suitable,
    InsufficientRuns {
        available_runs: u64,
        required_runs: u64,
    },
    Unavailable,
}

/// `current_observation` is a live lookup (only meaningful when
/// `snapshot.source_observation_id` is `Some`) -- a BPC's remaining runs
/// are real, mutable, current state, not something frozen at Plan
/// generation time. `None` when the snapshot named an observation that no
/// longer exists as a *current* asset (superseded by a later sync, or the
/// asset left the owner's hangar).
#[must_use]
pub fn classify_blueprint_availability(
    snapshot: Option<&BlueprintSnapshot>,
    current_observation: Option<&BlueprintObservation>,
) -> BlueprintAvailability {
    let Some(snapshot) = snapshot else {
        return BlueprintAvailability::NotAssigned;
    };
    if snapshot.source_mode != BlueprintSourceMode::ObservedAsset {
        return BlueprintAvailability::Assumed;
    }
    let Some(observation) = current_observation else {
        return BlueprintAvailability::Unavailable;
    };
    match observation.licensed_runs {
        Some(available_runs) if available_runs < snapshot.requested_runs => {
            BlueprintAvailability::InsufficientRuns {
                available_runs,
                required_runs: snapshot.requested_runs,
            }
        }
        _ => BlueprintAvailability::Suitable,
    }
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum BlueprintError {
    #[error("blueprint selection is required")]
    SelectionRequired,
    #[error("manual blueprint kind must be Original or Copy")]
    ManualKindUnknown,
    #[error("blueprint ME must be an integer from 0 through 10")]
    InvalidMaterialEfficiency,
    #[error("blueprint TE must be an integer from 0 through 20")]
    InvalidTimeEfficiency,
    #[error("a manual blueprint copy requires positive licensed runs")]
    CopyRunsRequired,
    #[error("blueprint copy has {licensed_runs} licensed runs, but this Build requests {requested_runs}")]
    CopyRunsInsufficient {
        licensed_runs: u64,
        requested_runs: u64,
    },
    #[error("blueprint observation was not found")]
    ObservationNotFound,
    #[error("blueprint observation belongs to another Workspace or Owner")]
    ObservationOwnerMismatch,
    #[error("observed blueprint type does not match the Build recipe")]
    ObservationTypeMismatch,
}

#[allow(clippy::too_many_arguments)]
pub fn capture_manual_snapshot(
    build_id: BuildId,
    blueprint_type_id: i64,
    blueprint_name: &str,
    requested_runs: u64,
    kind: BlueprintKind,
    material_efficiency: u8,
    time_efficiency: u8,
    licensed_runs: Option<u64>,
    notes: &str,
    now: DateTime<Utc>,
) -> Result<BlueprintSnapshot, BlueprintError> {
    validate(
        kind,
        material_efficiency,
        time_efficiency,
        licensed_runs,
        requested_runs,
        true,
    )?;
    Ok(BlueprintSnapshot {
        id: Uuid::new_v4(),
        build_id,
        source_mode: BlueprintSourceMode::Manual,
        blueprint_type_id,
        blueprint_name: blueprint_name.to_string(),
        kind,
        material_efficiency,
        time_efficiency,
        licensed_runs,
        requested_runs,
        source_observation_id: None,
        source_eve_item_id: None,
        source_owner_id: None,
        source_owner_name: None,
        source_location_id: None,
        source_location_name: None,
        observed_at: None,
        imported_at: None,
        manual_notes: (!notes.trim().is_empty()).then(|| notes.trim().to_string()),
        planned_duration_seconds: None,
        formula_version: "blueprint-snapshot-v1".into(),
        captured_at: now,
    })
}

pub fn capture_observed_snapshot(
    build_id: BuildId,
    owner_id: OwnerId,
    blueprint_type_id: i64,
    requested_runs: u64,
    observation: &BlueprintObservation,
    now: DateTime<Utc>,
) -> Result<BlueprintSnapshot, BlueprintError> {
    if observation.owner_id != owner_id {
        return Err(BlueprintError::ObservationOwnerMismatch);
    }
    if observation.blueprint_type_id != blueprint_type_id {
        return Err(BlueprintError::ObservationTypeMismatch);
    }
    validate(
        observation.kind,
        observation.material_efficiency,
        observation.time_efficiency,
        observation.licensed_runs,
        requested_runs,
        false,
    )?;
    Ok(BlueprintSnapshot {
        id: Uuid::new_v4(),
        build_id,
        source_mode: BlueprintSourceMode::ObservedAsset,
        blueprint_type_id,
        blueprint_name: observation.blueprint_name.clone(),
        kind: observation.kind,
        material_efficiency: observation.material_efficiency,
        time_efficiency: observation.time_efficiency,
        licensed_runs: observation.licensed_runs,
        requested_runs,
        source_observation_id: Some(observation.id),
        source_eve_item_id: Some(observation.eve_item_id),
        source_owner_id: Some(observation.owner_id),
        source_owner_name: Some(observation.owner_name.clone()),
        source_location_id: Some(observation.location_id),
        source_location_name: observation.location_name.clone(),
        observed_at: Some(observation.observed_at),
        imported_at: Some(observation.imported_at),
        manual_notes: None,
        planned_duration_seconds: None,
        formula_version: "blueprint-snapshot-v1".into(),
        captured_at: now,
    })
}

pub(crate) fn validate(
    kind: BlueprintKind,
    me: u8,
    te: u8,
    licensed: Option<u64>,
    requested: u64,
    manual: bool,
) -> Result<(), BlueprintError> {
    if me > 10 {
        return Err(BlueprintError::InvalidMaterialEfficiency);
    }
    if te > 20 {
        return Err(BlueprintError::InvalidTimeEfficiency);
    }
    if manual && kind == BlueprintKind::Unknown {
        return Err(BlueprintError::ManualKindUnknown);
    }
    if kind == BlueprintKind::Copy {
        let runs = licensed.filter(|value| *value > 0);
        if manual && runs.is_none() {
            return Err(BlueprintError::CopyRunsRequired);
        }
        if let Some(runs) = runs {
            if requested > runs {
                return Err(BlueprintError::CopyRunsInsufficient {
                    licensed_runs: runs,
                    requested_runs: requested,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // `BlueprintAvailability` must carry `tag = "kind"` (like
    // `TicketBlocker`): without it serde's default externally-tagged
    // representation serializes unit variants as bare strings
    // (`"notAssigned"`) instead of `{"kind":"notAssigned"}`, which breaks
    // the frontend's `availability.kind === "..."` checks.
    #[test]
    fn availability_serializes_with_a_kind_tag_matching_ticket_blocker() {
        assert_eq!(
            serde_json::to_value(BlueprintAvailability::NotAssigned).unwrap(),
            serde_json::json!({"kind": "notAssigned"})
        );
        assert_eq!(
            serde_json::to_value(BlueprintAvailability::Suitable).unwrap(),
            serde_json::json!({"kind": "suitable"})
        );
        assert_eq!(
            serde_json::to_value(BlueprintAvailability::InsufficientRuns {
                available_runs: 3,
                required_runs: 8,
            })
            .unwrap(),
            serde_json::json!({"kind": "insufficientRuns", "availableRuns": 3, "requiredRuns": 8})
        );
    }

    #[test]
    fn observed_selection_uses_the_camel_case_api_contract() {
        let observation_id = Uuid::new_v4();
        let selection: BlueprintSelection = serde_json::from_value(serde_json::json!({
            "mode": "observedAsset",
            "observationId": observation_id,
        }))
        .unwrap();
        assert_eq!(
            selection,
            BlueprintSelection::ObservedAsset {
                observation_id,
                kind: BlueprintKind::Unknown,
                material_efficiency: 0,
                time_efficiency: 0,
                licensed_runs: None,
            }
        );
        assert!(!selection.is_effective_config_captured());
    }

    #[test]
    fn observed_selection_with_captured_config_roundtrips_through_json() {
        let observation_id = Uuid::new_v4();
        let selection = BlueprintSelection::ObservedAsset {
            observation_id,
            kind: BlueprintKind::Copy,
            material_efficiency: 10,
            time_efficiency: 20,
            licensed_runs: None,
        };
        assert!(selection.is_effective_config_captured());
        let wire = serde_json::to_value(&selection).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "mode": "observedAsset",
                "observationId": observation_id,
                "kind": "copy",
                "materialEfficiency": 10,
                "timeEfficiency": 20,
            })
        );
        let roundtripped: BlueprintSelection = serde_json::from_value(wire).unwrap();
        assert_eq!(roundtripped, selection);
    }

    #[test]
    fn manual_selection_is_always_considered_captured() {
        let selection = BlueprintSelection::Manual {
            kind: BlueprintKind::Original,
            material_efficiency: 10,
            time_efficiency: 20,
            licensed_runs: None,
            notes: String::new(),
        };
        assert!(selection.is_effective_config_captured());
    }

    #[test]
    fn manual_copy_limits_and_unknown_are_explicit() {
        let id = BuildId::new();
        assert!(capture_manual_snapshot(
            id,
            1,
            "BP",
            1,
            BlueprintKind::Original,
            10,
            20,
            None,
            "",
            Utc::now()
        )
        .is_ok());
        assert!(matches!(
            capture_manual_snapshot(
                id,
                1,
                "BP",
                8,
                BlueprintKind::Copy,
                10,
                20,
                Some(7),
                "",
                Utc::now()
            ),
            Err(BlueprintError::CopyRunsInsufficient { .. })
        ));
        assert_eq!(
            capture_manual_snapshot(
                id,
                1,
                "BP",
                1,
                BlueprintKind::Unknown,
                0,
                0,
                None,
                "",
                Utc::now()
            ),
            Err(BlueprintError::ManualKindUnknown)
        );
    }
    fn observation(kind: BlueprintKind, licensed_runs: Option<u64>) -> BlueprintObservation {
        BlueprintObservation {
            id: Uuid::new_v4(),
            workspace_id: WorkspaceId::new(),
            owner_id: OwnerId::new(),
            owner_name: "Owner".into(),
            eve_item_id: 1,
            blueprint_type_id: 1,
            blueprint_name: "BP".into(),
            kind,
            material_efficiency: 10,
            time_efficiency: 20,
            licensed_runs,
            location_id: 1,
            location_flag: "Hangar".into(),
            location_name: None,
            observed_at: Utc::now(),
            imported_at: Utc::now(),
        }
    }

    fn manual_snapshot() -> BlueprintSnapshot {
        capture_manual_snapshot(
            BuildId::new(),
            1,
            "BP",
            10,
            BlueprintKind::Original,
            10,
            20,
            None,
            "",
            Utc::now(),
        )
        .unwrap()
    }

    fn observed_snapshot(
        requested_runs: u64,
        observation: &BlueprintObservation,
    ) -> BlueprintSnapshot {
        capture_observed_snapshot(
            BuildId::new(),
            observation.owner_id,
            1,
            requested_runs,
            observation,
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn no_snapshot_is_not_assigned() {
        assert_eq!(
            classify_blueprint_availability(None, None),
            BlueprintAvailability::NotAssigned
        );
    }

    #[test]
    fn manual_snapshot_is_assumed_never_checked() {
        let snapshot = manual_snapshot();
        assert_eq!(
            classify_blueprint_availability(Some(&snapshot), None),
            BlueprintAvailability::Assumed
        );
    }

    #[test]
    fn observed_snapshot_with_no_current_observation_is_unavailable() {
        let obs = observation(BlueprintKind::Copy, Some(10));
        let snapshot = observed_snapshot(5, &obs);
        assert_eq!(
            classify_blueprint_availability(Some(&snapshot), None),
            BlueprintAvailability::Unavailable
        );
    }

    #[test]
    fn observed_bpc_with_insufficient_current_runs_is_flagged() {
        let obs = observation(BlueprintKind::Copy, Some(10));
        let snapshot = observed_snapshot(8, &obs);
        let current = observation(BlueprintKind::Copy, Some(3));
        assert_eq!(
            classify_blueprint_availability(Some(&snapshot), Some(&current)),
            BlueprintAvailability::InsufficientRuns {
                available_runs: 3,
                required_runs: 8,
            }
        );
    }

    #[test]
    fn observed_bpo_is_suitable_regardless_of_requested_runs() {
        let obs = observation(BlueprintKind::Original, None);
        let snapshot = observed_snapshot(500, &obs);
        assert_eq!(
            classify_blueprint_availability(Some(&snapshot), Some(&obs)),
            BlueprintAvailability::Suitable
        );
    }

    #[test]
    fn observed_bpc_with_sufficient_current_runs_is_suitable() {
        let obs = observation(BlueprintKind::Copy, Some(10));
        let snapshot = observed_snapshot(8, &obs);
        let current = observation(BlueprintKind::Copy, Some(8));
        assert_eq!(
            classify_blueprint_availability(Some(&snapshot), Some(&current)),
            BlueprintAvailability::Suitable
        );
    }

    #[test]
    fn efficiency_is_not_clamped() {
        assert_eq!(
            capture_manual_snapshot(
                BuildId::new(),
                1,
                "BP",
                1,
                BlueprintKind::Original,
                11,
                0,
                None,
                "",
                Utc::now()
            ),
            Err(BlueprintError::InvalidMaterialEfficiency)
        );
    }
}
