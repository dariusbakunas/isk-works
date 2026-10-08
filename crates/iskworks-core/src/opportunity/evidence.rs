//! Warning and evidence-freshness projection: the `OpportunityWarning*`
//! shapes candidates carry, plus `classify_evidence` / `adjusted_price_readiness`
//! which map an observation timestamp to a fresh/stale/missing status. Pure
//! domain policy.

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunityCompleteness {
    Complete,
    Incomplete,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunityWarningKind {
    MissingMaterialPrice,
    MissingOutputPrice,
    InsufficientMarketDepth,
    IncompleteInstallationCost,
    MultipleOutputsUnsupported,
    StaleMarketEvidence,
    ThinOutputBook,
    /// Reserved, not yet produced by `project_candidate` -- deliberately
    /// deferred: thin-book warnings are emitted only where existing depth
    /// evidence makes them direct to derive. `derive_evidence_quality`
    /// already classifies it as `Qualified` so a future material-side thin
    /// check needs no policy change, only a producer.
    ThinInputBook,
    IncompleteEivBasis,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityMissingMaterial {
    pub type_id: i64,
    pub type_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityEivBasis {
    pub complete: bool,
    pub required_material_count: u64,
    pub observed_material_count: u64,
    pub missing_materials: Vec<OpportunityMissingMaterial>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OpportunityWarningDetails {
    StaleMarketEvidence {
        observed_at: DateTime<Utc>,
        age_seconds: u64,
        freshness_target_seconds: u64,
        refresh_state: OpportunityEvidenceState,
    },
    ThinBook {
        reasons: Vec<crate::OpportunityThinBookReason>,
    },
    IncompleteEivBasis {
        missing_materials: Vec<OpportunityMissingMaterial>,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityWarning {
    pub kind: OpportunityWarningKind,
    pub message: String,
    pub type_ids: Vec<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<OpportunityWarningDetails>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunityEvidenceState {
    Fresh,
    Stale,
    Missing,
    Pending,
    Failed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityEvidenceStatus {
    pub state: OpportunityEvidenceState,
    pub usable: bool,
    pub observed_at: Option<DateTime<Utc>>,
    pub age_seconds: Option<u64>,
    pub refresh_pending: bool,
    pub last_refresh_error: Option<String>,
}

#[must_use]
pub fn classify_evidence(
    observed_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    freshness: chrono::Duration,
) -> OpportunityEvidenceStatus {
    let Some(observed_at) = observed_at else {
        return OpportunityEvidenceStatus {
            state: OpportunityEvidenceState::Missing,
            usable: false,
            observed_at: None,
            age_seconds: None,
            refresh_pending: false,
            last_refresh_error: None,
        };
    };
    let age = now
        .signed_duration_since(observed_at)
        .max(chrono::Duration::zero());
    OpportunityEvidenceStatus {
        state: if age <= freshness {
            OpportunityEvidenceState::Fresh
        } else {
            OpportunityEvidenceState::Stale
        },
        usable: true,
        observed_at: Some(observed_at),
        age_seconds: u64::try_from(age.num_seconds()).ok(),
        refresh_pending: false,
        last_refresh_error: None,
    }
}

pub(super) fn adjusted_price_readiness(
    required_count: usize,
    available_count: usize,
    observed_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    freshness: chrono::Duration,
) -> OpportunityEvidenceStatus {
    if required_count == 0 {
        return OpportunityEvidenceStatus {
            state: OpportunityEvidenceState::Fresh,
            usable: true,
            observed_at: None,
            age_seconds: None,
            refresh_pending: false,
            last_refresh_error: None,
        };
    }
    let mut status = classify_evidence(observed_at, now, freshness);
    if available_count < required_count {
        status.state = OpportunityEvidenceState::Missing;
        status.usable = false;
    }
    status
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn evidence_age_marks_old_observations_stale_without_making_them_unusable() {
        let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
        let observed_at = now - chrono::Duration::hours(12);

        let evidence = classify_evidence(Some(observed_at), now, chrono::Duration::minutes(15));

        assert_eq!(evidence.state, OpportunityEvidenceState::Stale);
        assert!(evidence.usable);
        assert_eq!(evidence.observed_at, Some(observed_at));
        assert_eq!(evidence.age_seconds, Some(43_200));
    }

    #[test]
    fn absent_observation_is_missing_and_unusable() {
        let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();

        let evidence = classify_evidence(None, now, chrono::Duration::minutes(15));

        assert_eq!(evidence.state, OpportunityEvidenceState::Missing);
        assert!(!evidence.usable);
        assert_eq!(evidence.age_seconds, None);
    }

    #[test]
    fn partial_adjusted_price_dataset_is_missing_even_when_recent() {
        let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
        let observed_at = now - chrono::Duration::minutes(1);

        let evidence =
            adjusted_price_readiness(4, 3, Some(observed_at), now, chrono::Duration::hours(6));

        assert_eq!(evidence.state, OpportunityEvidenceState::Missing);
        assert!(!evidence.usable);
        assert_eq!(evidence.observed_at, Some(observed_at));
    }
}
