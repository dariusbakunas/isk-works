use super::*;

pub use iskworks_core::{
    AdjustedPrice, AssetObservation, BlueprintAssetObservation, CharacterIndustryJobObservation,
    EsiResponseMetadata, EveEntityName, StructureInformation, WalletJournalObservation,
    WalletTransactionObservation,
};

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum EsiError {
    #[error("ESI integration configuration error: {0}")]
    Configuration(&'static str),
    #[error("authorization is required")]
    AuthorizationRequired,
    #[error("the EVE connection is missing a required scope")]
    MissingScope,
    #[error("the connected EVE character cannot access this structure")]
    AccessDenied,
    #[error("EVE temporarily rate limited requests")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("EVE's error limit is nearly exhausted")]
    EsiErrorLimit { reset_seconds: Option<u64> },
    /// Tranquility's daily downtime: the request was never sent (see
    /// `downtime`).
    #[error("EVE is in its daily downtime")]
    ServerDowntime { retry_after_seconds: Option<u64> },
    #[error("EVE is temporarily unavailable")]
    TemporaryFailure,
    #[error("EVE rejected the request")]
    PermanentFailure,
    #[error("EVE returned an invalid response")]
    InvalidResponse,
    #[error("the EVE identity token could not be validated")]
    InvalidIdentity,
    #[error("secret encryption failed")]
    SecretEncryption,
    #[error("encrypted token material is invalid or corrupted")]
    SecretDecryption,
}

#[derive(Clone, Eq, PartialEq)]
pub struct AuthenticatedToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub identity: Identity,
    /// The JWT `owner` claim: a hash identifying the EVE account that owns
    /// the character. It changes when the character is transferred (e.g.
    /// sold on the Character Bazaar) while `character_id` stays the same.
    pub owner_hash: Option<String>,
}

impl std::fmt::Debug for AuthenticatedToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthenticatedToken")
            .field("access_token", &"[redacted]")
            .field("refresh_token", &"[redacted]")
            .field("expires_at", &self.expires_at)
            .field("identity", &self.identity)
            .field("owner_hash", &self.owner_hash)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct RefreshedToken {
    pub access_token: String,
    pub rotated_refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub identity: Identity,
}

impl std::fmt::Debug for RefreshedToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RefreshedToken")
            .field("access_token", &"[redacted]")
            .field(
                "rotated_refresh_token",
                &self.rotated_refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .field("expires_at", &self.expires_at)
            .field("identity", &self.identity)
            .finish()
    }
}

impl EsiError {
    /// How long ESI asked us to wait before trying again: a 429's
    /// `Retry-After`, the error-limit reset, or the rest of daily downtime.
    #[must_use]
    pub fn retry_after(&self) -> Option<std::time::Duration> {
        match self {
            Self::RateLimited {
                retry_after_seconds: Some(seconds),
            }
            | Self::EsiErrorLimit {
                reset_seconds: Some(seconds),
            }
            | Self::ServerDowntime {
                retry_after_seconds: Some(seconds),
            } => Some(std::time::Duration::from_secs(*seconds)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Identity {
    pub character_id: i64,
    pub character_name: String,
    pub scopes: BTreeSet<String>,
}

/// Whether ESI is reachable right now, as far as users need to know.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EsiAvailability {
    /// Tranquility is in its daily downtime; ESI calls are paused.
    pub downtime: bool,
    /// When to ask again, while in downtime.
    pub retry_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EsiResponse<T> {
    pub records: Vec<T>,
    pub not_modified: bool,
    pub metadata: EsiResponseMetadata,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WalletBalanceObservation {
    #[serde(with = "rust_decimal::serde::str")]
    pub balance: Decimal,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IndustrySystemCostIndex {
    pub solar_system_id: i64,
    pub manufacturing: Decimal,
    pub reaction: Decimal,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct MarketOrderObservation {
    pub order_id: i64,
    pub type_id: i64,
    pub location_id: i64,
    pub system_id: i64,
    pub is_buy_order: bool,
    pub price: Decimal,
    pub volume_remain: u64,
    pub volume_total: u64,
    pub min_volume: u64,
    pub order_range: String,
    pub issued_at: DateTime<Utc>,
    pub duration_days: u32,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterPublicInfo {
    pub character_id: i64,
    pub name: String,
    pub corporation_id: i64,
    pub security_status: Option<Decimal>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterLocationObservation {
    pub solar_system_id: i64,
    pub station_id: Option<i64>,
    pub structure_id: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterSkillEntry {
    pub skill_id: i64,
    pub active_skill_level: i64,
    pub trained_skill_level: i64,
    pub skillpoints_in_skill: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterSkillsObservation {
    pub total_sp: i64,
    pub unallocated_sp: Option<i64>,
    pub skills: Vec<CharacterSkillEntry>,
}

/// `GET /characters/{id}/attributes/`: the five training attributes
/// (including any active implant bonuses, as ESI reports them) and remap
/// state.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterAttributesObservation {
    pub charisma: i64,
    pub intelligence: i64,
    pub memory: i64,
    pub perception: i64,
    pub willpower: i64,
    pub bonus_remaps: Option<i64>,
    pub last_remap_date: Option<DateTime<Utc>>,
    pub accrued_remap_cooldown_date: Option<DateTime<Utc>>,
}

/// One entry from `/characters/{id}/skillqueue/`.
///
/// `start_date`/`finish_date` are `None` for every entry at once when ESI
/// reports a paused queue (no active training clock) — never for a subset
/// of entries. A response with some entries dated and others not is
/// malformed/inconsistent data, not a legitimate paused-queue
/// representation.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterSkillQueueEntry {
    pub skill_id: i64,
    pub finished_level: i64,
    pub queue_position: i64,
    pub start_date: Option<DateTime<Utc>>,
    pub finish_date: Option<DateTime<Utc>>,
    pub training_start_sp: Option<i64>,
    pub level_start_sp: Option<i64>,
    pub level_end_sp: Option<i64>,
}

/// One row of `GET /characters/{id}/orders/`: an active order the character
/// placed. ESI omits `is_buy_order`, `escrow` and `min_volume` on sell
/// orders; they read as `false`, `None` and `1`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterOrderObservation {
    pub order_id: i64,
    pub type_id: i64,
    pub region_id: i64,
    pub location_id: i64,
    pub is_buy_order: bool,
    pub is_corporation: bool,
    pub price: Decimal,
    pub volume_remain: u64,
    pub volume_total: u64,
    pub min_volume: u64,
    /// ESI's spelling: `station`, `solarsystem`, `region` or a jump count.
    pub order_range: String,
    pub issued_at: DateTime<Utc>,
    pub duration_days: u32,
    pub escrow: Option<Decimal>,
}

/// A character's answer to an in-game calendar event.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarEventResponse {
    Accepted,
    Declined,
    Tentative,
    NotResponded,
}

impl CalendarEventResponse {
    /// ESI's spelling, also the stored value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Tentative => "tentative",
            Self::NotResponded => "not_responded",
        }
    }

    /// Unknown values read as not responded rather than failing the sync.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "accepted" => Self::Accepted,
            "declined" => Self::Declined,
            "tentative" => Self::Tentative,
            _ => Self::NotResponded,
        }
    }
}

/// One row of `GET /characters/{id}/calendar/`: an upcoming event summary.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CalendarEventObservation {
    pub event_id: i64,
    pub event_date: DateTime<Utc>,
    pub title: String,
    pub importance: i64,
    pub response: CalendarEventResponse,
}

/// `GET /characters/{id}/calendar/{event_id}/`. `text` is player-authored
/// markup, kept raw.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CalendarEventDetailObservation {
    pub event_id: i64,
    pub date: DateTime<Utc>,
    pub title: String,
    pub importance: i64,
    pub duration_minutes: i64,
    pub owner_id: Option<i64>,
    pub owner_name: Option<String>,
    pub owner_type: Option<String>,
    pub text: String,
}

/// One row of `GET /characters/{id}/planets/`: a PI colony header.
/// `last_update` is when the owner last touched the colony in game -- the
/// pin contents in [`CharacterPlanetDetailObservation`] are a snapshot as of
/// that moment, not live.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterPlanetObservation {
    pub planet_id: i64,
    pub planet_type: String,
    pub solar_system_id: i64,
    pub upgrade_level: i64,
    pub num_pins: i64,
    pub last_update: DateTime<Utc>,
}

/// `GET /characters/{id}/planets/{planet_id}/`, reduced to the pins (routes
/// and links are not needed to derive throughput: factory schematics and
/// extractor programs fully determine it).
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CharacterPlanetDetailObservation {
    pub pins: Vec<PlanetPinObservation>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct PlanetPinObservation {
    pub pin_id: i64,
    pub type_id: i64,
    /// Factory schematic (top-level `schematic_id`, or the nested
    /// `factory_details.schematic_id` ESI sometimes uses instead).
    pub schematic_id: Option<i64>,
    pub contents: Vec<PlanetPinContentObservation>,
    pub install_time: Option<DateTime<Utc>>,
    pub expiry_time: Option<DateTime<Utc>>,
    pub last_cycle_start: Option<DateTime<Utc>>,
    pub extractor: Option<PlanetExtractorObservation>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub struct PlanetPinContentObservation {
    pub type_id: i64,
    pub amount: i64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub struct PlanetExtractorObservation {
    pub product_type_id: i64,
    pub qty_per_cycle: i64,
    pub cycle_time_seconds: i64,
    pub head_count: i64,
}
