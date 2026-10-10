//! EVE SSO and ESI protocol boundary for ISK Works.

mod client_identity;
mod crypto;
mod downtime;
mod error_limit;
mod metrics;
mod oauth;
mod rate_limit;
#[cfg(any(test, feature = "test-support"))]
mod test_support;
mod transport;

pub use client_identity::contact_configured;
pub use crypto::{EncryptedSecret, SecretCipher};
pub use oauth::{
    authorization_url, hash_state, new_pkce, OAuthState, PkceVerifier, ASSET_SCOPE,
    BLUEPRINT_SCOPE, INDUSTRY_JOBS_SCOPE, LOCATION_SCOPE, MARKET_STRUCTURE_SCOPE, PLANETS_SCOPE,
    SKILLS_SCOPE, SKILL_QUEUE_SCOPE, STRUCTURE_SCOPE, WALLET_SCOPE,
};
#[cfg(any(test, feature = "test-support"))]
pub use test_support::UnusedEsiTransport;
pub use transport::{
    AdjustedPrice, AssetObservation, AuthenticatedToken, BlueprintAssetObservation,
    CharacterIndustryJobObservation, CharacterLocationObservation,
    CharacterPlanetDetailObservation, CharacterPlanetObservation, CharacterPublicInfo,
    CharacterSkillEntry, CharacterSkillQueueEntry, CharacterSkillsObservation, EsiAvailability,
    EsiError, EsiResponse, EsiResponseMetadata, EsiTransport, EveEntityName, HttpEsiTransport,
    Identity, IndustrySystemCostIndex, MarketOrderObservation, PlanetExtractorObservation,
    PlanetPinContentObservation, PlanetPinObservation, RefreshedToken, StructureInformation,
    WalletBalanceObservation, WalletJournalObservation, WalletTransactionObservation,
};
