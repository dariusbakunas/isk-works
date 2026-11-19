//! Which worksheet components the workspace owns a manufacturing blueprint
//! for, and whether that blueprint can run the planned producer job.
//!
//! Pure: the coordinator loads the observations and the SDE product ->
//! blueprint mapping; this module only classifies them. Reporting only --
//! it never changes sourcing or a producer's blueprint selection.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::{BuildWorksheetRow, WorksheetSourcing};
use crate::{BlueprintKind, BlueprintObservation, BlueprintSelection};

/// A row's blueprint evidence. Present only on Buy and Manufacturing rows
/// whose type has a manufacturing blueprint in the SDE.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetBlueprintCoverage {
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub holdings: BlueprintHoldingSummary,
    /// Manufacturing rows only: the producer job this row is built by.
    pub producer: Option<ProducerBlueprintUse>,
    pub state: BlueprintCoverageState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintHoldingSummary {
    pub originals: u32,
    pub copies: u32,
    /// Observations whose kind ESI sync could not classify.
    pub unknown: u32,
    /// Licensed runs summed across copies; a copy without known runs adds 0.
    pub copy_runs: u64,
    /// Highest ME, then TE, then an original before a copy.
    pub best: Option<OwnedBlueprintRef>,
    /// Newest observation time -- the holdings are as of this sync.
    pub observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedBlueprintRef {
    pub observation_id: Uuid,
    pub kind: BlueprintKind,
    pub material_efficiency: u8,
    pub time_efficiency: u8,
    pub licensed_runs: Option<u64>,
    pub location_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerBlueprintUse {
    pub required_runs: u64,
    pub source: ProducerBlueprintSource,
    /// The observation an `ObservedAsset` selection was captured from.
    pub observation_id: Option<Uuid>,
    pub material_efficiency: Option<u8>,
    pub time_efficiency: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProducerBlueprintSource {
    /// Manual ME/TE assumptions, or no selection at all.
    Manual,
    ObservedAsset,
}

impl ProducerBlueprintSource {
    #[must_use]
    pub fn from_selection(selection: Option<&BlueprintSelection>) -> (Self, Option<Uuid>) {
        match selection {
            Some(BlueprintSelection::ObservedAsset { observation_id, .. }) => {
                (Self::ObservedAsset, Some(*observation_id))
            }
            Some(BlueprintSelection::Manual { .. }) | None => (Self::Manual, None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BlueprintCoverageState {
    /// An original, or copies (with enough runs, on a Manufacturing row).
    Owned,
    /// Manufacturing row, copies only, fewer runs than the producer needs.
    InsufficientRuns,
    /// Only blueprints whose kind is unknown; can't judge runs.
    UnknownKind,
    NotOwned,
}

/// Blueprint evidence for one product type, as the coordinator loaded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorksheetBlueprintHoldings {
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub holdings: BlueprintHoldingSummary,
}

/// Header counts, each blueprint type counted once however many rows
/// (consumers, groups) it appears on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetBlueprintSummary {
    pub owned: u32,
    /// Manufacturing rows with no blueprint, or too few runs.
    pub missing: u32,
    /// Buy rows whose blueprint is owned.
    pub owned_but_bought: u32,
    /// Manufacturing rows with an owned blueprint, but the producer plans
    /// on manual ME/TE.
    pub producer_uses_manual: u32,
}

#[must_use]
pub fn summarize_holdings(observations: &[BlueprintObservation]) -> BlueprintHoldingSummary {
    let mut summary = BlueprintHoldingSummary::default();
    let mut best: Option<&BlueprintObservation> = None;
    for observation in observations {
        match observation.kind {
            BlueprintKind::Original => summary.originals += 1,
            BlueprintKind::Copy => {
                summary.copies += 1;
                summary.copy_runs = summary
                    .copy_runs
                    .saturating_add(observation.licensed_runs.unwrap_or(0));
            }
            BlueprintKind::Unknown => summary.unknown += 1,
        }
        if best.is_none_or(|current| rank(observation) > rank(current)) {
            best = Some(observation);
        }
        if summary
            .observed_at
            .is_none_or(|newest| observation.observed_at > newest)
        {
            summary.observed_at = Some(observation.observed_at);
        }
    }
    summary.best = best.map(|observation| OwnedBlueprintRef {
        observation_id: observation.id,
        kind: observation.kind,
        material_efficiency: observation.material_efficiency,
        time_efficiency: observation.time_efficiency,
        licensed_runs: observation.licensed_runs,
        location_name: observation.location_name.clone(),
    });
    summary
}

fn rank(observation: &BlueprintObservation) -> (u8, u8, u8) {
    let kind = match observation.kind {
        BlueprintKind::Original => 2,
        BlueprintKind::Copy => 1,
        BlueprintKind::Unknown => 0,
    };
    (
        observation.material_efficiency,
        observation.time_efficiency,
        kind,
    )
}

#[must_use]
pub fn coverage_state(
    sourcing: WorksheetSourcing,
    holdings: &BlueprintHoldingSummary,
    producer: Option<&ProducerBlueprintUse>,
) -> BlueprintCoverageState {
    if holdings.originals > 0 {
        return BlueprintCoverageState::Owned;
    }
    let required_runs = producer
        .filter(|_| sourcing == WorksheetSourcing::Manufacturing)
        .map(|producer| producer.required_runs);
    let copies_suffice =
        holdings.copies > 0 && required_runs.is_none_or(|required| holdings.copy_runs >= required);
    if copies_suffice {
        return BlueprintCoverageState::Owned;
    }
    if holdings.unknown > 0 {
        return BlueprintCoverageState::UnknownKind;
    }
    if holdings.copies > 0 {
        return BlueprintCoverageState::InsufficientRuns;
    }
    BlueprintCoverageState::NotOwned
}

#[derive(Default)]
struct BlueprintFlags {
    owned: bool,
    missing: bool,
    owned_but_bought: bool,
    producer_uses_manual: bool,
}

#[must_use]
pub fn summarize_rows<'a>(
    rows: impl IntoIterator<Item = &'a BuildWorksheetRow>,
) -> WorksheetBlueprintSummary {
    let mut by_blueprint = BTreeMap::<i64, BlueprintFlags>::new();
    for row in rows {
        let Some(coverage) = &row.blueprint else {
            continue;
        };
        let flags = by_blueprint.entry(coverage.blueprint_type_id).or_default();
        let owned = coverage.state == BlueprintCoverageState::Owned;
        flags.owned |= owned;
        match row.sourcing {
            WorksheetSourcing::Manufacturing => {
                flags.missing |= matches!(
                    coverage.state,
                    BlueprintCoverageState::NotOwned | BlueprintCoverageState::InsufficientRuns
                );
                flags.producer_uses_manual |= owned
                    && coverage
                        .producer
                        .as_ref()
                        .is_some_and(|producer| producer.source == ProducerBlueprintSource::Manual);
            }
            WorksheetSourcing::Buy => flags.owned_but_bought |= owned,
            WorksheetSourcing::Reaction | WorksheetSourcing::Unresolved => {}
        }
    }
    let count = |pick: fn(&BlueprintFlags) -> bool| {
        u32::try_from(by_blueprint.values().filter(|flags| pick(flags)).count()).unwrap_or(u32::MAX)
    };
    WorksheetBlueprintSummary {
        owned: count(|flags| flags.owned),
        missing: count(|flags| flags.missing),
        owned_but_bought: count(|flags| flags.owned_but_bought),
        producer_uses_manual: count(|flags| flags.producer_uses_manual),
    }
}

#[cfg(test)]
mod tests;
