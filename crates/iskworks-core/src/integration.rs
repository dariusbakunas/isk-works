use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{MarketRefreshState, Money, OwnerId, WorkspaceId};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectedCharacterId(pub Uuid);

impl ConnectedCharacterId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConnectedCharacterId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionStatus {
    Connected,
    NeedsReconnection,
    MissingScope,
    TemporarilyUnavailable,
    Disconnected,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedCharacter {
    pub id: ConnectedCharacterId,
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    pub eve_character_id: i64,
    pub character_name: String,
    pub status: ConnectionStatus,
    pub granted_scopes: Vec<String>,
    pub access_token_expires_at: Option<DateTime<Utc>>,
    pub last_refreshed_at: Option<DateTime<Utc>>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub connected_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub disconnected_at: Option<DateTime<Utc>>,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CharacterSourceKind {
    CharacterInfo,
    Location,
    Skills,
    Wallet,
    IndustryJobs,
    /// Full ESI asset snapshot (`EsiSyncKind::Assets`) -- the Inventory ESI
    /// holdings drill-down depends on this data being fresh.
    Assets,
    /// ESI wallet transaction history (`EsiSyncKind::WalletTransactions`) --
    /// distinct from `Wallet`, which only tracks the balance snapshot.
    WalletTransactions,
    /// PI colonies (`/characters/{id}/planets/` + per-planet layouts). The
    /// scope is optional, so this source never degrades the roster badge
    /// (see [`CharacterSourceKind::is_optional`]).
    Planets,
}

impl CharacterSourceKind {
    #[must_use]
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::CharacterInfo => "character_info",
            Self::Location => "location",
            Self::Skills => "skills",
            Self::Wallet => "wallet",
            Self::IndustryJobs => "industry_jobs",
            Self::Assets => "assets",
            Self::WalletTransactions => "wallet_transactions",
            Self::Planets => "planets",
        }
    }

    #[must_use]
    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "character_info" => Some(Self::CharacterInfo),
            "location" => Some(Self::Location),
            "skills" => Some(Self::Skills),
            "wallet" => Some(Self::Wallet),
            "industry_jobs" => Some(Self::IndustryJobs),
            "assets" => Some(Self::Assets),
            "wallet_transactions" => Some(Self::WalletTransactions),
            "planets" => Some(Self::Planets),
            _ => None,
        }
    }

    #[must_use]
    pub fn all() -> [Self; 8] {
        [
            Self::CharacterInfo,
            Self::Location,
            Self::Skills,
            Self::Wallet,
            Self::IndustryJobs,
            Self::Assets,
            Self::WalletTransactions,
            Self::Planets,
        ]
    }

    /// Sources backed by an optional ESI scope. Many characters never run PI,
    /// so a missing scope or a stale/failed fetch here is a per-feature
    /// concern, not a character-health one.
    #[must_use]
    pub fn is_optional(self) -> bool {
        matches!(self, Self::Planets)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSourceSyncState {
    pub connection_id: ConnectedCharacterId,
    pub source_kind: CharacterSourceKind,
    pub refresh_state: MarketRefreshState,
    pub summary: Option<serde_json::Value>,
    pub observed_at: Option<DateTime<Utc>>,
    pub last_attempted_at: Option<DateTime<Utc>>,
    pub next_refresh_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CharacterHealth {
    Healthy,
    Stale,
    PartialPerms,
    ReconnectRequired,
    SyncPending,
}

/// How stale a source is allowed to get, relative to its own refresh
/// cadence, before it taints the whole character's roster badge -- two
/// missed cycles, not one, so a single slow worker pass doesn't flip a
/// healthy character to "Stale" the moment it's marginally overdue.
#[must_use]
fn source_stale_after(kind: CharacterSourceKind) -> chrono::Duration {
    let refresh_interval = match kind {
        CharacterSourceKind::CharacterInfo => chrono::Duration::hours(24),
        CharacterSourceKind::Location => chrono::Duration::minutes(10),
        CharacterSourceKind::Skills => chrono::Duration::minutes(60),
        CharacterSourceKind::Wallet => chrono::Duration::minutes(15),
        CharacterSourceKind::IndustryJobs => chrono::Duration::minutes(30),
        // Coarser than the other sources: assets can be a 100k+-row,
        // multi-page fetch and wallet transactions their own paginated
        // fetch, both well within ESI's own ~1h cache window for these
        // endpoints -- polling faster wouldn't surface newer data anyway.
        CharacterSourceKind::Assets => chrono::Duration::minutes(60),
        CharacterSourceKind::WalletTransactions => chrono::Duration::minutes(30),
        CharacterSourceKind::Planets => chrono::Duration::minutes(30),
    };
    refresh_interval * 2
}

/// Rolls a connection's OAuth/scope health and its five per-source sync
/// states up into the single badge the roster card shows. Priority order:
/// a broken connection always wins over stale data, since reconnecting is
/// the actionable fix; a per-source "missing scope: ..." failure (see
/// `CharacterSyncService`) is also a permissions problem even though the
/// connection-level status is otherwise `Connected`.
#[must_use]
pub fn character_health(
    connection_status: ConnectionStatus,
    sources: &[CharacterSourceSyncState],
    now: DateTime<Utc>,
) -> CharacterHealth {
    match connection_status {
        ConnectionStatus::NeedsReconnection
        | ConnectionStatus::Disconnected
        | ConnectionStatus::TemporarilyUnavailable => return CharacterHealth::ReconnectRequired,
        ConnectionStatus::MissingScope => return CharacterHealth::PartialPerms,
        ConnectionStatus::Connected => {}
    }
    let sources = sources
        .iter()
        .filter(|source| !source.source_kind.is_optional())
        .collect::<Vec<_>>();
    if sources.iter().any(
        |source| matches!(&source.last_error, Some(error) if error.starts_with("missing scope:")),
    ) {
        return CharacterHealth::PartialPerms;
    }
    if sources
        .iter()
        .all(|source| source.refresh_state == MarketRefreshState::Missing)
    {
        return CharacterHealth::SyncPending;
    }
    let any_stale = sources.iter().any(|source| match source.observed_at {
        None => source.refresh_state == MarketRefreshState::Failed,
        Some(observed_at) => {
            now.signed_duration_since(observed_at) > source_stale_after(source.source_kind)
        }
    });
    if any_stale {
        CharacterHealth::Stale
    } else {
        CharacterHealth::Healthy
    }
}

// EVE skill type_ids that gate manufacturing/reaction/research job slots --
// verified against the imported SDE's `sde_types` table, not trusted from
// memory (an earlier guess for Mass Reactions was wrong: 45746 instead of
// the real 45748).
pub const MASS_PRODUCTION_SKILL_ID: i64 = 3387;
pub const ADVANCED_MASS_PRODUCTION_SKILL_ID: i64 = 24625;
pub const LABORATORY_OPERATION_SKILL_ID: i64 = 3406;
pub const ADVANCED_LABORATORY_OPERATION_SKILL_ID: i64 = 24624;
pub const MASS_REACTIONS_SKILL_ID: i64 = 45748;
pub const ADVANCED_MASS_REACTIONS_SKILL_ID: i64 = 45749;

/// EVE's manufacturing/reaction/research job-slot cap: one free slot per
/// category, plus one more per level of the category's paired skill and its
/// "Advanced" counterpart (e.g. Mass Production + Advanced Mass Production
/// for manufacturing).
///
/// The three categories are independent pools, each gated by its own skill
/// pair:
/// * manufacturing -- Mass Production + Advanced Mass Production
/// * reactions     -- Mass Reactions + Advanced Mass Reactions
/// * research/science -- Laboratory Operation + Advanced Laboratory
///   Operation, a *single* pool shared by ME research, TE research, copying,
///   invention and reverse engineering (EVE has no separate per-science-type
///   slot skill).
#[must_use]
pub fn industry_job_slot_max(primary_level: i64, advanced_level: i64) -> u64 {
    (1 + primary_level + advanced_level).max(1) as u64
}

/// The three independent job-slot pools a running industry job can occupy.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IndustrySlotBucket {
    Manufacturing,
    Reaction,
    Research,
}

/// EVE industry `activity_id` -> the activity kind ISK Works surfaces in the
/// character Industry tab. Ids are ESI's own, from
/// `GET /characters/{id}/industry/jobs/`:
///
/// | id | ESI activity                    | `IndustryActivity`           | slot bucket   |
/// |----|--------------------------------|------------------------------|---------------|
/// | 1  | Manufacturing                  | `Manufacturing`              | manufacturing |
/// | 3  | Researching Time Efficiency    | `TimeEfficiencyResearch`     | research      |
/// | 4  | Researching Material Efficiency| `MaterialEfficiencyResearch` | research      |
/// | 5  | Copying                        | `Copying`                    | research      |
/// | 7  | Reverse Engineering            | `ReverseEngineering`         | research      |
/// | 8  | Invention                      | `Invention`                  | research      |
/// | 9  | Reactions                      | `Reaction`                   | reaction      |
///
/// Ids 2 ("Researching Technology") and 6 ("Duplicating") were removed from
/// EVE years ago and never appear in live responses; any unrecognised id
/// falls through to `Other`, which is treated as a science job (the safest
/// bucket -- it never inflates the manufacturing or reaction counts).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IndustryActivity {
    Manufacturing,
    TimeEfficiencyResearch,
    MaterialEfficiencyResearch,
    Copying,
    ReverseEngineering,
    Invention,
    Reaction,
    Other,
}

impl IndustryActivity {
    #[must_use]
    pub fn from_activity_id(activity_id: i64) -> Self {
        match activity_id {
            1 => Self::Manufacturing,
            3 => Self::TimeEfficiencyResearch,
            4 => Self::MaterialEfficiencyResearch,
            5 => Self::Copying,
            7 => Self::ReverseEngineering,
            8 => Self::Invention,
            9 => Self::Reaction,
            _ => Self::Other,
        }
    }

    /// Which of the three independent job-slot pools this activity draws
    /// from (see `industry_job_slot_max`). Every science activity shares the
    /// research pool; `Other` is bucketed there too.
    #[must_use]
    pub fn slot_bucket(self) -> IndustrySlotBucket {
        match self {
            Self::Manufacturing => IndustrySlotBucket::Manufacturing,
            Self::Reaction => IndustrySlotBucket::Reaction,
            Self::TimeEfficiencyResearch
            | Self::MaterialEfficiencyResearch
            | Self::Copying
            | Self::ReverseEngineering
            | Self::Invention
            | Self::Other => IndustrySlotBucket::Research,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EsiSyncKind {
    Assets,
    WalletTransactions,
    AllSupported,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EsiSyncStatus {
    Pending,
    Running,
    Succeeded,
    PartiallySucceeded,
    Failed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EsiSyncRunId(pub Uuid);

impl EsiSyncRunId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EsiSyncRunId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EsiSyncRun {
    pub id: EsiSyncRunId,
    pub connection_id: ConnectedCharacterId,
    pub requested_kind: EsiSyncKind,
    pub status: EsiSyncStatus,
    pub phase: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cache_expires_at: Option<DateTime<Utc>>,
    pub imported_count: u64,
    pub unchanged_count: u64,
    pub skipped_count: u64,
    pub error_count: u64,
    pub error_code: Option<String>,
    pub summary: String,
}

pub fn exact_total(unit_price: Decimal, quantity: u64) -> Option<Money> {
    Money::parse(&unit_price.checked_mul(Decimal::from(quantity))?.to_string()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_source_kind_round_trips_through_its_db_string() {
        for kind in CharacterSourceKind::all() {
            assert_eq!(
                CharacterSourceKind::from_db_str(kind.as_db_str()),
                Some(kind)
            );
        }
        assert_eq!(CharacterSourceKind::from_db_str("bogus"), None);
    }

    #[test]
    fn industry_job_slot_max_adds_one_base_slot_plus_both_skill_levels() {
        // No relevant skills trained -> the one free slot everyone has.
        assert_eq!(industry_job_slot_max(0, 0), 1);
        // Partial: Mass Production III, no Advanced -> 1 + 3 + 0.
        assert_eq!(industry_job_slot_max(3, 0), 4);
        // Common maxed manufacturing/reactions: level V + IV -> 10.
        assert_eq!(industry_job_slot_max(5, 4), 10);
        // Fully maxed (both to V), e.g. Laboratory Operation V + Advanced V
        // -> 11 science slots, the single pool ME/TE/copy/invention share.
        assert_eq!(industry_job_slot_max(5, 5), 11);
    }

    #[test]
    fn industry_activity_classifies_every_esi_activity_id_into_its_slot_bucket() {
        use IndustryActivity::*;
        use IndustrySlotBucket::{
            Manufacturing as MfgSlot, Reaction as RxnSlot, Research as ResSlot,
        };

        let cases = [
            (1, Manufacturing, MfgSlot),
            (3, TimeEfficiencyResearch, ResSlot),
            (4, MaterialEfficiencyResearch, ResSlot),
            (5, Copying, ResSlot),
            (7, ReverseEngineering, ResSlot),
            (8, Invention, ResSlot),
            (9, Reaction, RxnSlot),
        ];
        for (id, activity, bucket) in cases {
            assert_eq!(IndustryActivity::from_activity_id(id), activity, "id {id}");
            assert_eq!(activity.slot_bucket(), bucket, "id {id} bucket");
        }
        // Removed/unknown ids never appear live but must not panic or leak
        // into the manufacturing/reaction counts.
        assert_eq!(IndustryActivity::from_activity_id(2), Other);
        assert_eq!(IndustryActivity::from_activity_id(6), Other);
        assert_eq!(IndustryActivity::from_activity_id(0), Other);
        assert_eq!(Other.slot_bucket(), IndustrySlotBucket::Research);
    }

    fn source(
        kind: CharacterSourceKind,
        refresh_state: MarketRefreshState,
        observed_at: Option<DateTime<Utc>>,
        last_error: Option<&str>,
    ) -> CharacterSourceSyncState {
        CharacterSourceSyncState {
            connection_id: ConnectedCharacterId::new(),
            source_kind: kind,
            refresh_state,
            summary: None,
            observed_at,
            last_attempted_at: None,
            next_refresh_at: None,
            last_error: last_error.map(str::to_string),
        }
    }

    fn all_fresh(now: DateTime<Utc>) -> Vec<CharacterSourceSyncState> {
        CharacterSourceKind::all()
            .into_iter()
            .map(|kind| source(kind, MarketRefreshState::Current, Some(now), None))
            .collect()
    }

    #[test]
    fn a_broken_connection_always_wins_over_stale_data() {
        let now = Utc::now();
        for status in [
            ConnectionStatus::NeedsReconnection,
            ConnectionStatus::Disconnected,
            ConnectionStatus::TemporarilyUnavailable,
        ] {
            assert_eq!(
                character_health(status, &all_fresh(now), now),
                CharacterHealth::ReconnectRequired
            );
        }
    }

    #[test]
    fn connection_level_missing_scope_yields_partial_perms() {
        let now = Utc::now();
        assert_eq!(
            character_health(ConnectionStatus::MissingScope, &all_fresh(now), now),
            CharacterHealth::PartialPerms
        );
    }

    #[test]
    fn a_per_source_missing_scope_failure_yields_partial_perms_even_when_connected() {
        let now = Utc::now();
        let mut sources = all_fresh(now);
        sources.push(source(
            CharacterSourceKind::IndustryJobs,
            MarketRefreshState::Failed,
            None,
            Some("missing scope: esi-industry.read_character_jobs.v1"),
        ));
        assert_eq!(
            character_health(ConnectionStatus::Connected, &sources, now),
            CharacterHealth::PartialPerms
        );
    }

    #[test]
    fn an_optional_planets_source_never_degrades_health() {
        let now = Utc::now();
        let mut sources = all_fresh(now);
        sources.retain(|source| source.source_kind != CharacterSourceKind::Planets);
        sources.push(source(
            CharacterSourceKind::Planets,
            MarketRefreshState::Failed,
            None,
            Some("missing scope: esi-planets.manage_planets.v1"),
        ));
        assert_eq!(
            character_health(ConnectionStatus::Connected, &sources, now),
            CharacterHealth::Healthy
        );
    }

    #[test]
    fn every_source_still_missing_yields_sync_pending() {
        let now = Utc::now();
        let sources: Vec<_> = CharacterSourceKind::all()
            .into_iter()
            .map(|kind| source(kind, MarketRefreshState::Missing, None, None))
            .collect();
        assert_eq!(
            character_health(ConnectionStatus::Connected, &sources, now),
            CharacterHealth::SyncPending
        );
    }

    #[test]
    fn a_source_stale_beyond_two_refresh_intervals_taints_the_badge() {
        let now = Utc::now();
        let mut sources = all_fresh(now);
        sources.push(source(
            CharacterSourceKind::Wallet,
            MarketRefreshState::Current,
            Some(now - chrono::Duration::hours(1)),
            None,
        ));
        assert_eq!(
            character_health(ConnectionStatus::Connected, &sources, now),
            CharacterHealth::Stale
        );
    }

    #[test]
    fn a_non_scope_failure_without_any_observation_yet_reads_as_stale_not_healthy() {
        let now = Utc::now();
        let mut sources = all_fresh(now);
        sources.push(source(
            CharacterSourceKind::Location,
            MarketRefreshState::Failed,
            None,
            Some("temporary ESI error"),
        ));
        assert_eq!(
            character_health(ConnectionStatus::Connected, &sources, now),
            CharacterHealth::Stale
        );
    }

    #[test]
    fn everything_fresh_and_connected_is_healthy() {
        let now = Utc::now();
        assert_eq!(
            character_health(ConnectionStatus::Connected, &all_fresh(now), now),
            CharacterHealth::Healthy
        );
    }
}
