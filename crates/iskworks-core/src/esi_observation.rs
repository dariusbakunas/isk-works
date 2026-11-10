//! What ISK Works records from ESI: asset, blueprint, wallet, structure,
//! entity-name, adjusted-price and industry-job observations, plus the
//! response metadata a sync run keeps. `iskworks-esi` parses ESI responses
//! into these and re-exports them; storage persists them without depending
//! on the ESI client.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Eq, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EsiResponseMetadata {
    pub etag: Option<String>,
    pub expires: Option<String>,
    pub last_modified: Option<String>,
    pub pages: Option<u32>,
    pub error_limit_remain: Option<u32>,
    pub error_limit_reset: Option<u32>,
}

impl EsiResponseMetadata {
    /// ESI's `Expires`: the earliest this resource may be requested again.
    /// Refetching before it circumvents ESI's cache, which CCP treats as
    /// abuse.
    #[must_use]
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc2822(value).ok())
            .map(|value| value.with_timezone(&Utc))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct AssetObservation {
    pub item_id: i64,
    pub type_id: i64,
    pub quantity: i64,
    pub location_id: i64,
    pub location_type: String,
    pub location_flag: String,
    pub is_singleton: bool,
    pub is_blueprint_copy: Option<bool>,
    pub raw: Value,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct BlueprintAssetObservation {
    pub item_id: i64,
    pub type_id: i64,
    pub location_id: i64,
    pub location_flag: String,
    pub material_efficiency: i16,
    pub time_efficiency: i16,
    pub runs: i64,
    pub quantity: i64,
    pub raw: Value,
}

impl BlueprintAssetObservation {
    /// Highest material efficiency a blueprint can be researched to.
    pub const MAX_MATERIAL_EFFICIENCY: i16 = 10;
    /// Highest time efficiency a blueprint can be researched to.
    pub const MAX_TIME_EFFICIENCY: i16 = 20;

    /// Whether ME and TE are within the game's ranges (`0..=10`, `0..=20`).
    /// ESI has been seen to report a blueprint outside them; such a record
    /// is unusable, not a reason to fail the rest of the sync.
    #[must_use]
    pub fn efficiency_in_range(&self) -> bool {
        (0..=Self::MAX_MATERIAL_EFFICIENCY).contains(&self.material_efficiency)
            && (0..=Self::MAX_TIME_EFFICIENCY).contains(&self.time_efficiency)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct WalletTransactionObservation {
    pub transaction_id: i64,
    pub client_id: i64,
    pub location_id: i64,
    pub type_id: i64,
    pub quantity: i64,
    pub unit_price: Decimal,
    pub is_buy: bool,
    pub is_personal: bool,
    pub journal_ref_id: i64,
    pub transacted_at: DateTime<Utc>,
    pub raw: Value,
}

/// One `wallet/journal` entry. `amount` is signed (negative = ISK out) and
/// `balance` is the wallet balance right after the entry, which is what makes
/// a true balance history possible.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct WalletJournalObservation {
    pub ref_id: i64,
    pub date: DateTime<Utc>,
    pub ref_type: String,
    pub amount: Decimal,
    pub balance: Option<Decimal>,
    pub first_party_id: Option<i64>,
    pub second_party_id: Option<i64>,
    pub context_id: Option<i64>,
    pub context_id_type: Option<String>,
    pub description: Option<String>,
    pub reason: Option<String>,
    pub tax: Option<Decimal>,
    pub tax_receiver_id: Option<i64>,
    pub raw: Value,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct StructureInformation {
    pub structure_id: i64,
    pub name: String,
    pub owner_id: i64,
    pub solar_system_id: i64,
    pub type_id: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct EveEntityName {
    pub id: i64,
    pub name: String,
    pub category: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AdjustedPrice {
    pub type_id: i64,
    pub adjusted_price: Decimal,
}

/// One row of `GET /characters/{id}/industry/jobs/?include_completed=true`:
/// running jobs (`active`, `paused`, `ready`) plus the last 90 days of
/// finished ones (`delivered`, `cancelled`, `reverted`). `blueprint_id` is
/// kept so a job can be matched to the Blueprint library (its ME/TE);
/// `installer_id`, `blueprint_location_id` and `output_location_id` are the
/// deliberate omissions: ISK Works has no location-tree reconciliation for
/// character jobs.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterIndustryJobObservation {
    pub job_id: i64,
    pub activity_id: i64,
    /// The installed blueprint's item id.
    pub blueprint_id: Option<i64>,
    pub blueprint_type_id: i64,
    /// The item this job yields: the built product (manufacturing), the
    /// reaction output, or -- for research/copy/invention -- the blueprint
    /// itself. Absent on some historical rows.
    pub product_type_id: Option<i64>,
    pub facility_id: i64,
    /// Set only when the job runs in an NPC station; `None` for player
    /// structures (where `facility_id` is the structure id).
    pub station_id: Option<i64>,
    pub runs: i64,
    pub licensed_runs: Option<i64>,
    pub cost: Option<Decimal>,
    /// Invention/reverse-engineering success chance in `0.0..=1.0`; `None`
    /// for activities that always succeed.
    pub probability: Option<Decimal>,
    pub duration_seconds: Option<i64>,
    pub status: String,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub pause_date: Option<DateTime<Utc>>,
    pub completed_date: Option<DateTime<Utc>>,
    pub completed_character_id: Option<i64>,
    pub successful_runs: Option<i64>,
}
