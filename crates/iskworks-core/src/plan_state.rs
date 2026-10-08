//! A root plan's optimistic-concurrency evidence: every Build's revision
//! (and draft timestamp), which a canonical write echoes back to prove it was
//! decided against the plan as it is now.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::production_dependency::RootPlanRecords;
use crate::BuildId;

/// One Build's persisted state as far as producer configuration is
/// concerned. `draft_updated_at` is included because a facility-profile
/// deletion rewrites draft planning without bumping the Build revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanBuildState {
    pub build_id: BuildId,
    pub revision: u64,
    pub draft_updated_at: Option<DateTime<Utc>>,
}

/// Every Build of a root plan, sorted by id.
#[must_use]
pub fn plan_state(records: &RootPlanRecords) -> Vec<PlanBuildState> {
    let mut states: Vec<PlanBuildState> = std::iter::once(&records.root)
        .chain(&records.producers)
        .map(|build| PlanBuildState {
            build_id: build.id,
            revision: build.revision,
            draft_updated_at: build.draft_planning.as_ref().map(|draft| draft.updated_at),
        })
        .collect();
    states.sort_by_key(|state| state.build_id.0);
    states
}
