use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use iskworks_core::{
    character_health, industry_job_slot_max, CharacterHealth, CharacterSourceKind,
    CharacterSourceSyncState, ConnectedCharacter, ConnectedCharacterId, ConnectionStatus,
    IndustryActivity, IndustrySlotBucket, InventoryError, MarketRefreshState, WorkspaceId,
    ADVANCED_LABORATORY_OPERATION_SKILL_ID, ADVANCED_MASS_PRODUCTION_SKILL_ID,
    ADVANCED_MASS_REACTIONS_SKILL_ID, LABORATORY_OPERATION_SKILL_ID, MASS_PRODUCTION_SKILL_ID,
    MASS_REACTIONS_SKILL_ID,
};
use iskworks_esi::SKILL_QUEUE_SCOPE;
use iskworks_sde::SdeReadRepository;
use iskworks_storage::{FacilityLabel, PgEsiRepository};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillQueueEntryDto {
    pub skill_id: i64,
    pub skill_name: Option<String>,
    pub finished_level: i64,
    pub queue_position: i64,
    pub start_date: Option<DateTime<Utc>>,
    pub finish_date: Option<DateTime<Utc>>,
    pub training_start_sp: Option<i64>,
    /// SP threshold at the start of this queued level. Together with
    /// `level_end_sp` it gives the level's full SP cost; the frontend uses
    /// it to compute how many SP the queue still has to train.
    pub level_start_sp: Option<i64>,
    pub level_end_sp: Option<i64>,
    /// The character's *current* permanently-trained level in this skill
    /// (ESI `trained_skill_level` from the synced `skills` array), or 0
    /// when the skill is synced but untrained. `None` only when the skills
    /// array itself is absent. Lets the Skills-tab level indicators show
    /// already-trained vs. queued-but-untrained levels from authoritative
    /// state rather than inferring it from queue position.
    pub current_trained_level: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterRosterEntry {
    pub connection_id: ConnectedCharacterId,
    pub eve_character_id: i64,
    pub character_name: String,
    pub corporation_id: Option<i64>,
    pub corporation_name: Option<String>,
    pub security_status: Option<Decimal>,
    pub solar_system_id: Option<i64>,
    pub solar_system_name: Option<String>,
    pub wallet_balance: Option<Decimal>,
    pub total_sp: Option<i64>,
    /// Free SP not yet applied to any skill (ESI `unallocated_sp`).
    pub unallocated_sp: Option<i64>,
    pub training_queue: Vec<SkillQueueEntryDto>,
    pub training_observed_at: Option<DateTime<Utc>>,
    /// The connection has not granted `esi-skills.read_skillqueue.v1`, so
    /// `training_queue` is empty for lack of authorization rather than an
    /// empty queue. The `skills` source still succeeds (total SP comes from
    /// a different scope), so this cannot be read off `sources[].lastError`
    /// the way other missing scopes are.
    pub training_queue_scope_missing: bool,
    pub manufacturing_active_jobs: Option<u64>,
    pub manufacturing_max_jobs: Option<u64>,
    pub reaction_active_jobs: Option<u64>,
    pub reaction_max_jobs: Option<u64>,
    pub research_active_jobs: Option<u64>,
    pub research_max_jobs: Option<u64>,
    pub connection_status: ConnectionStatus,
    pub health: CharacterHealth,
    pub last_synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSourceDetail {
    pub source_kind: CharacterSourceKind,
    pub refresh_state: MarketRefreshState,
    pub observed_at: Option<DateTime<Utc>>,
    pub next_refresh_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

/// One slot-occupying industry job (`active`, `paused` or `ready`) for the
/// inspector's Industry tab, with type and facility ids already resolved to
/// names where possible. `startDate`/`endDate` are ESI's own fixed
/// timestamps -- remaining time and progress are derived client-side from
/// them, never persisted (see the Industry tab's per-minute tick).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndustryJobDto {
    pub job_id: i64,
    /// The classified activity (`IndustryActivity`), so the frontend never
    /// has to know raw ESI `activity_id`s. `activity_id` is kept alongside
    /// for debugging / forward compatibility.
    pub activity: IndustryActivity,
    pub activity_id: i64,
    /// ESI job status verbatim: `active`, `paused` or `ready`.
    pub status: String,
    pub blueprint_type_id: i64,
    pub blueprint_name: Option<String>,
    pub product_type_id: Option<i64>,
    pub product_name: Option<String>,
    pub runs: i64,
    pub facility_id: i64,
    pub facility_name: Option<String>,
    pub solar_system_name: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterDetail {
    #[serde(flatten)]
    pub entry: CharacterRosterEntry,
    pub sources: Vec<CharacterSourceDetail>,
    /// Slot-occupying industry jobs, soonest-finishing first. Empty when the
    /// `industry_jobs` source has never synced, the character has no running
    /// jobs, or the connection lacks the industry-jobs scope (the caller
    /// distinguishes these via `sources`).
    pub industry_jobs: Vec<IndustryJobDto>,
}

#[derive(Clone)]
pub struct CharacterRosterService {
    repository: Arc<PgEsiRepository>,
    sde_repository: Arc<dyn SdeReadRepository>,
}

impl CharacterRosterService {
    #[must_use]
    pub fn new(
        repository: Arc<PgEsiRepository>,
        sde_repository: Arc<dyn SdeReadRepository>,
    ) -> Self {
        Self {
            repository,
            sde_repository,
        }
    }

    /// Every connected (not disconnected) character in the workspace, as a
    /// compact roster card each. Pure read-model assembly over what PRs 1-4
    /// already sync and persist -- no ESI calls happen here, including for
    /// corporation/solar-system names: those are looked up in the existing
    /// `eve_entity_names` cache (already used for wallet-client-name
    /// resolution) and simply come back `None` if nothing has resolved that
    /// ID yet, rather than triggering a live lookup from a read endpoint.
    pub async fn list(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<CharacterRosterEntry>, InventoryError> {
        let connections: Vec<ConnectedCharacter> = self
            .repository
            .list_connections(workspace_id)
            .await?
            .into_iter()
            .filter(|connection| connection.disconnected_at.is_none())
            .collect();

        let mut sources_by_connection = HashMap::with_capacity(connections.len());
        let mut entity_ids = HashSet::new();
        let mut skill_ids = HashSet::new();
        for connection in &connections {
            let sources = self
                .repository
                .character_source_state(connection.id)
                .await?;
            collect_entity_ids(&sources, &mut entity_ids);
            collect_training_skill_ids(&sources, &mut skill_ids);
            sources_by_connection.insert(connection.id, sources);
        }
        let names = self.resolve_names(entity_ids).await?;
        let skill_names = self.resolve_skill_names(skill_ids).await;

        let now = Utc::now();
        Ok(connections
            .into_iter()
            .map(|connection| {
                let sources = sources_by_connection
                    .remove(&connection.id)
                    .unwrap_or_default();
                assemble_entry(connection, &sources, &names, &skill_names, now)
            })
            .collect())
    }

    /// The fuller detail view for one character's inspector: the same
    /// roster fields plus every source's raw sync state, for the Sync tab.
    pub async fn detail(
        &self,
        connection_id: ConnectedCharacterId,
    ) -> Result<CharacterDetail, InventoryError> {
        let connection = self.repository.get_connection(connection_id).await?;
        let sources = self
            .repository
            .character_source_state(connection_id)
            .await?;
        let mut entity_ids = HashSet::new();
        collect_entity_ids(&sources, &mut entity_ids);
        let names = self.resolve_names(entity_ids).await?;
        let mut skill_ids = HashSet::new();
        collect_training_skill_ids(&sources, &mut skill_ids);
        let skill_names = self.resolve_skill_names(skill_ids).await;

        // Industry-tab job list: resolve blueprint/product type ids against
        // the SDE type catalogue and facility ids against whatever this
        // workspace has already cached. All best-effort -- an unresolved name
        // renders as a fallback, never an error.
        let workspace_id = connection.workspace_id;
        let jobs_summary =
            find(&sources, CharacterSourceKind::IndustryJobs).and_then(|s| s.summary.as_ref());
        let (industry_type_ids, facility_ids) = collect_industry_job_ids(jobs_summary);
        let industry_type_names = self.resolve_type_names(&industry_type_ids).await;
        let facility_labels = self
            .repository
            .industry_facility_labels(workspace_id, &facility_ids)
            .await?;
        let industry_jobs =
            parse_industry_jobs(jobs_summary, &industry_type_names, &facility_labels);

        let now = Utc::now();
        let entry = assemble_entry(connection, &sources, &names, &skill_names, now);
        let sources = sources
            .iter()
            .map(|source| CharacterSourceDetail {
                source_kind: source.source_kind,
                refresh_state: source.refresh_state,
                observed_at: source.observed_at,
                next_refresh_at: source.next_refresh_at,
                last_error: source.last_error.clone(),
            })
            .collect();
        Ok(CharacterDetail {
            entry,
            sources,
            industry_jobs,
        })
    }

    async fn resolve_names(
        &self,
        entity_ids: HashSet<i64>,
    ) -> Result<HashMap<i64, String>, InventoryError> {
        let ids: Vec<i64> = entity_ids.into_iter().collect();
        self.repository.entity_names(&ids).await
    }

    /// Skill names come from the local SDE type catalog, not ESI -- a
    /// lookup failure here is a data-availability hiccup, not a reason to
    /// fail the whole roster, so it's swallowed the same way corp/system
    /// name resolution is (see `CharacterSyncService::resolve_and_cache_names`).
    async fn resolve_skill_names(&self, skill_ids: HashSet<i64>) -> HashMap<i64, String> {
        let ids: Vec<i64> = skill_ids.into_iter().collect();
        self.resolve_type_names(&ids).await
    }

    /// Bulk type_id -> `name_en` from the local SDE catalogue (skills,
    /// blueprints, products). Same swallow-on-failure contract as
    /// `resolve_skill_names`: a lookup hiccup drops names, it never fails the
    /// request.
    async fn resolve_type_names(&self, ids: &[i64]) -> HashMap<i64, String> {
        if ids.is_empty() {
            return HashMap::new();
        }
        match self.sde_repository.type_names(ids).await {
            Ok(names) => names.into_iter().collect(),
            Err(error) => {
                tracing::warn!(%error, "failed to resolve SDE type names");
                HashMap::new()
            }
        }
    }
}

fn collect_entity_ids(sources: &[CharacterSourceSyncState], ids: &mut HashSet<i64>) {
    for source in sources {
        let Some(summary) = &source.summary else {
            continue;
        };
        match source.source_kind {
            CharacterSourceKind::CharacterInfo => {
                if let Some(corporation_id) = summary.get("corporation_id").and_then(Value::as_i64)
                {
                    ids.insert(corporation_id);
                }
            }
            CharacterSourceKind::Location => {
                if let Some(solar_system_id) =
                    summary.get("solar_system_id").and_then(Value::as_i64)
                {
                    ids.insert(solar_system_id);
                }
            }
            CharacterSourceKind::Skills
            | CharacterSourceKind::Wallet
            | CharacterSourceKind::IndustryJobs
            | CharacterSourceKind::Assets
            | CharacterSourceKind::WalletTransactions
            | CharacterSourceKind::Planets => {}
        }
    }
}

fn collect_training_skill_ids(sources: &[CharacterSourceSyncState], ids: &mut HashSet<i64>) {
    let Some(summary) = find(sources, CharacterSourceKind::Skills).and_then(|s| s.summary.as_ref())
    else {
        return;
    };
    let Some(queue) = summary.get("skillQueue").and_then(Value::as_array) else {
        return;
    };
    for entry in queue {
        if let Some(skill_id) = entry.get("skill_id").and_then(Value::as_i64) {
            ids.insert(skill_id);
        }
    }
}

fn find(
    sources: &[CharacterSourceSyncState],
    kind: CharacterSourceKind,
) -> Option<&CharacterSourceSyncState> {
    sources.iter().find(|source| source.source_kind == kind)
}

fn decimal_field(summary: Option<&Value>, key: &str) -> Option<Decimal> {
    summary
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .and_then(|text| text.parse::<Decimal>().ok())
}

/// A trained skill's `active_skill_level` from the persisted `skills` array,
/// or 0 if the character has never trained it at all (distinct from the
/// skills source never having synced, which the caller handles separately
/// by leaving the whole slot-max `None`).
fn active_skill_level(skills_summary: &Value, skill_id: i64) -> i64 {
    skills_summary
        .get("skills")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|skill| skill.get("skill_id").and_then(Value::as_i64) == Some(skill_id))
        .and_then(|skill| skill.get("active_skill_level"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

/// The character's current permanently-trained level in `skill_id`, from
/// the synced `skills` array (ESI `trained_skill_level`). `Some(0)` when
/// the array is present but omits the skill (ESI omits fully-untrained
/// skills), `None` only when the array itself is absent. Distinct from
/// [`active_skill_level`], which reads `active_skill_level` and is used for
/// slot-count maths.
pub(crate) fn current_trained_level(skills_summary: Option<&Value>, skill_id: i64) -> Option<i64> {
    let skills = skills_summary?.get("skills").and_then(Value::as_array)?;
    let level = skills
        .iter()
        .find(|skill| skill.get("skill_id").and_then(Value::as_i64) == Some(skill_id))
        .and_then(|skill| skill.get("trained_skill_level"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    Some(level)
}

/// The whole cached `skillQueue` array, in whatever order ESI provided
/// (see `iskworks_esi`'s `queue_position`-ascending ordering contract) --
/// including entries whose `finish_date` has already passed. Deciding
/// what's currently training is the frontend's/worker's job
/// (`iskworks_core::training::derive_training_state`), not this read
/// model's.
fn parse_training_queue(
    skills_summary: Option<&Value>,
    skill_names: &HashMap<i64, String>,
) -> Vec<SkillQueueEntryDto> {
    let Some(queue) = skills_summary
        .and_then(|value| value.get("skillQueue"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    queue
        .iter()
        .filter_map(|entry| {
            let skill_id = entry.get("skill_id").and_then(Value::as_i64)?;
            let finished_level = entry.get("finished_level").and_then(Value::as_i64)?;
            let queue_position = entry.get("queue_position").and_then(Value::as_i64)?;
            Some(SkillQueueEntryDto {
                skill_id,
                skill_name: skill_names.get(&skill_id).cloned(),
                finished_level,
                queue_position,
                start_date: entry
                    .get("start_date")
                    .and_then(Value::as_str)
                    .and_then(|text| text.parse::<DateTime<Utc>>().ok()),
                finish_date: entry
                    .get("finish_date")
                    .and_then(Value::as_str)
                    .and_then(|text| text.parse::<DateTime<Utc>>().ok()),
                training_start_sp: entry.get("training_start_sp").and_then(Value::as_i64),
                level_start_sp: entry.get("level_start_sp").and_then(Value::as_i64),
                level_end_sp: entry.get("level_end_sp").and_then(Value::as_i64),
                current_trained_level: current_trained_level(skills_summary, skill_id),
            })
        })
        .collect()
}

struct IndustrySlotMax {
    manufacturing: u64,
    reaction: u64,
    research: u64,
}

fn industry_slot_max(skills_summary: &Value) -> IndustrySlotMax {
    IndustrySlotMax {
        manufacturing: industry_job_slot_max(
            active_skill_level(skills_summary, MASS_PRODUCTION_SKILL_ID),
            active_skill_level(skills_summary, ADVANCED_MASS_PRODUCTION_SKILL_ID),
        ),
        reaction: industry_job_slot_max(
            active_skill_level(skills_summary, MASS_REACTIONS_SKILL_ID),
            active_skill_level(skills_summary, ADVANCED_MASS_REACTIONS_SKILL_ID),
        ),
        research: industry_job_slot_max(
            active_skill_level(skills_summary, LABORATORY_OPERATION_SKILL_ID),
            active_skill_level(skills_summary, ADVANCED_LABORATORY_OPERATION_SKILL_ID),
        ),
    }
}

struct ActiveJobCounts {
    manufacturing: u64,
    reaction: u64,
    research: u64,
}

/// A job's ESI status occupies one of its category's concurrent slots. EVE
/// holds the slot for `paused` jobs and for `ready` jobs (timer finished,
/// awaiting delivery) exactly as it does for `active` ones -- only
/// `delivered`/`cancelled`/`reverted` free it. The summary cards and the
/// Active Jobs list both key off this, so "8 / 10 Manufacturing" always
/// matches the number of rows shown.
fn is_slot_occupying(status: &str) -> bool {
    matches!(status, "active" | "paused" | "ready")
}

/// Slot-occupying jobs bucketed into the three independent slot pools via
/// the shared `IndustryActivity` classifier (see
/// `iskworks_core::IndustryActivity`) -- one source of truth for
/// `activity_id` -> pool, reused by the Active Jobs list.
fn active_job_counts(jobs_summary: &Value) -> ActiveJobCounts {
    let mut counts = ActiveJobCounts {
        manufacturing: 0,
        reaction: 0,
        research: 0,
    };
    let Some(jobs) = jobs_summary.get("jobs").and_then(Value::as_array) else {
        return counts;
    };
    for job in jobs {
        let status = job
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !is_slot_occupying(status) {
            continue;
        }
        let Some(activity_id) = job.get("activity_id").and_then(Value::as_i64) else {
            continue;
        };
        match IndustryActivity::from_activity_id(activity_id).slot_bucket() {
            IndustrySlotBucket::Manufacturing => counts.manufacturing += 1,
            IndustrySlotBucket::Reaction => counts.reaction += 1,
            IndustrySlotBucket::Research => counts.research += 1,
        }
    }
    counts
}

/// Blueprint + product type ids (for SDE name resolution) and facility ids
/// (for location labels) across every slot-occupying job in the summary.
fn collect_industry_job_ids(jobs_summary: Option<&Value>) -> (Vec<i64>, Vec<i64>) {
    let mut type_ids = HashSet::new();
    let mut facility_ids = HashSet::new();
    if let Some(jobs) = jobs_summary
        .and_then(|value| value.get("jobs"))
        .and_then(Value::as_array)
    {
        for job in jobs {
            let status = job
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !is_slot_occupying(status) {
                continue;
            }
            if let Some(id) = job.get("blueprint_type_id").and_then(Value::as_i64) {
                type_ids.insert(id);
            }
            if let Some(id) = job.get("product_type_id").and_then(Value::as_i64) {
                type_ids.insert(id);
            }
            if let Some(id) = job.get("facility_id").and_then(Value::as_i64) {
                facility_ids.insert(id);
            }
        }
    }
    (
        type_ids.into_iter().collect(),
        facility_ids.into_iter().collect(),
    )
}

fn parse_date_field(job: &Value, key: &str) -> Option<DateTime<Utc>> {
    job.get(key)
        .and_then(Value::as_str)
        .and_then(|text| text.parse::<DateTime<Utc>>().ok())
}

/// Turn the persisted `industry_jobs` summary (`{ "jobs": [<raw ESI rows>],
/// ... }`) into the inspector DTO list: slot-occupying jobs only, classified
/// activity, names filled in where resolved, soonest-finishing first.
fn parse_industry_jobs(
    jobs_summary: Option<&Value>,
    type_names: &HashMap<i64, String>,
    facilities: &HashMap<i64, FacilityLabel>,
) -> Vec<IndustryJobDto> {
    let Some(jobs) = jobs_summary
        .and_then(|value| value.get("jobs"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut out: Vec<IndustryJobDto> = jobs
        .iter()
        .filter_map(|job| {
            let status = job.get("status").and_then(Value::as_str)?.to_string();
            if !is_slot_occupying(&status) {
                return None;
            }
            let job_id = job.get("job_id").and_then(Value::as_i64)?;
            let activity_id = job.get("activity_id").and_then(Value::as_i64)?;
            let blueprint_type_id = job.get("blueprint_type_id").and_then(Value::as_i64)?;
            let product_type_id = job.get("product_type_id").and_then(Value::as_i64);
            let facility_id = job
                .get("facility_id")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            let runs = job.get("runs").and_then(Value::as_i64).unwrap_or(1);
            let facility = facilities.get(&facility_id);
            Some(IndustryJobDto {
                job_id,
                activity: IndustryActivity::from_activity_id(activity_id),
                activity_id,
                status,
                blueprint_type_id,
                blueprint_name: type_names.get(&blueprint_type_id).cloned(),
                product_type_id,
                product_name: product_type_id.and_then(|id| type_names.get(&id).cloned()),
                runs,
                facility_id,
                facility_name: facility.map(|label| label.name.clone()),
                solar_system_name: facility.and_then(|label| label.solar_system_name.clone()),
                start_date: parse_date_field(job, "start_date"),
                end_date: parse_date_field(job, "end_date"),
            })
        })
        .collect();
    // Soonest-finishing first; a job with no parseable end date sorts last.
    out.sort_by(|a, b| match (a.end_date, b.end_date) {
        (Some(x), Some(y)) => x.cmp(&y).then(a.job_id.cmp(&b.job_id)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.job_id.cmp(&b.job_id),
    });
    out
}

fn assemble_entry(
    connection: ConnectedCharacter,
    sources: &[CharacterSourceSyncState],
    names: &HashMap<i64, String>,
    skill_names: &HashMap<i64, String>,
    now: DateTime<Utc>,
) -> CharacterRosterEntry {
    let info_summary =
        find(sources, CharacterSourceKind::CharacterInfo).and_then(|s| s.summary.as_ref());
    let location_summary =
        find(sources, CharacterSourceKind::Location).and_then(|s| s.summary.as_ref());
    let skills_summary =
        find(sources, CharacterSourceKind::Skills).and_then(|s| s.summary.as_ref());
    let wallet_summary =
        find(sources, CharacterSourceKind::Wallet).and_then(|s| s.summary.as_ref());
    let jobs_summary =
        find(sources, CharacterSourceKind::IndustryJobs).and_then(|s| s.summary.as_ref());

    let corporation_id = info_summary
        .and_then(|value| value.get("corporation_id"))
        .and_then(Value::as_i64);
    let security_status = decimal_field(info_summary, "security_status");
    let solar_system_id = location_summary
        .and_then(|value| value.get("solar_system_id"))
        .and_then(Value::as_i64);
    let wallet_balance = decimal_field(wallet_summary, "balance");
    let total_sp = skills_summary
        .and_then(|value| value.get("total_sp"))
        .and_then(Value::as_i64);
    let unallocated_sp = skills_summary
        .and_then(|value| value.get("unallocated_sp"))
        .and_then(Value::as_i64);
    let training_queue_scope_missing = !connection
        .granted_scopes
        .iter()
        .any(|scope| scope == SKILL_QUEUE_SCOPE);
    let training_queue = parse_training_queue(skills_summary, skill_names);
    let training_observed_at =
        find(sources, CharacterSourceKind::Skills).and_then(|s| s.observed_at);
    let slot_max = skills_summary.map(industry_slot_max);
    let job_counts = jobs_summary.map(active_job_counts);

    let last_synced_at = sources.iter().filter_map(|source| source.observed_at).max();
    let health = character_health(connection.status, sources, now);

    CharacterRosterEntry {
        connection_id: connection.id,
        eve_character_id: connection.eve_character_id,
        character_name: connection.character_name,
        corporation_id,
        corporation_name: corporation_id.and_then(|id| names.get(&id).cloned()),
        security_status,
        solar_system_id,
        solar_system_name: solar_system_id.and_then(|id| names.get(&id).cloned()),
        wallet_balance,
        total_sp,
        unallocated_sp,
        training_queue,
        training_observed_at,
        training_queue_scope_missing,
        manufacturing_active_jobs: job_counts.as_ref().map(|counts| counts.manufacturing),
        manufacturing_max_jobs: slot_max.as_ref().map(|max| max.manufacturing),
        reaction_active_jobs: job_counts.as_ref().map(|counts| counts.reaction),
        reaction_max_jobs: slot_max.as_ref().map(|max| max.reaction),
        research_active_jobs: job_counts.as_ref().map(|counts| counts.research),
        research_max_jobs: slot_max.as_ref().map(|max| max.research),
        connection_status: connection.status,
        health,
        last_synced_at,
    }
}

#[cfg(test)]
mod tests;
