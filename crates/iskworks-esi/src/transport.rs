use std::collections::BTreeSet;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use reqwest::{header, Client, StatusCode};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::client_identity::{process_user_agent, COMPATIBILITY_DATE};
use crate::downtime::{restarted_since_downtime, DowntimeDecision, DowntimeGuard};
use crate::error_limit::{ErrorLimitGuard, ERROR_LIMITED_STATUS};
use crate::rate_limit::{self, Caller, RateLimitGuard};
use crate::PkceVerifier;

mod classify;
mod http;
mod parse;
mod sso;
mod types;
use classify::*;
pub use http::*;
use parse::*;
use sso::*;
pub use types::*;

#[async_trait]
pub trait EsiTransport: Send + Sync {
    async fn exchange_code(
        &self,
        code: &str,
        verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError>;
    async fn refresh(&self, refresh_token: &str) -> Result<RefreshedToken, EsiError>;
    /// Revokes a refresh token at EVE SSO, so a disconnected character's
    /// grant stops working everywhere, not just in our database. Transports
    /// that never hold real tokens (fixtures, fakes) needn't do anything.
    async fn revoke_refresh_token(&self, refresh_token: &str) -> Result<(), EsiError> {
        let _ = refresh_token;
        Ok(())
    }
    async fn assets(
        &self,
        access_token: &str,
        character_id: i64,
        page: u32,
        etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError>;
    async fn blueprints(
        &self,
        access_token: &str,
        character_id: i64,
        page: u32,
    ) -> Result<EsiResponse<BlueprintAssetObservation>, EsiError> {
        let _ = (access_token, character_id, page);
        Err(EsiError::PermanentFailure)
    }
    async fn wallet_transactions(
        &self,
        access_token: &str,
        character_id: i64,
        from_id: Option<i64>,
        etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError>;
    async fn wallet_balance(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<WalletBalanceObservation>, EsiError> {
        let _ = (access_token, character_id);
        Err(EsiError::PermanentFailure)
    }
    /// One page of the wallet journal (ESI paginates by `page`, 1-based).
    async fn wallet_journal(
        &self,
        access_token: &str,
        character_id: i64,
        page: u32,
    ) -> Result<EsiResponse<WalletJournalObservation>, EsiError> {
        let _ = (access_token, character_id, page);
        Err(EsiError::PermanentFailure)
    }
    async fn universe_names(&self, ids: &[i64]) -> Result<Vec<EveEntityName>, EsiError> {
        let _ = ids;
        Err(EsiError::PermanentFailure)
    }
    async fn structure(
        &self,
        access_token: &str,
        structure_id: i64,
    ) -> Result<StructureInformation, EsiError>;
    async fn industry_systems(&self) -> Result<EsiResponse<IndustrySystemCostIndex>, EsiError>;
    async fn market_prices(&self) -> Result<EsiResponse<AdjustedPrice>, EsiError> {
        Err(EsiError::PermanentFailure)
    }
    async fn regional_market_orders(
        &self,
        region_id: i64,
        type_id: i64,
        page: u32,
        etag: Option<&str>,
    ) -> Result<EsiResponse<MarketOrderObservation>, EsiError> {
        let _ = (region_id, type_id, page, etag);
        Err(EsiError::PermanentFailure)
    }
    /// `GET /markets/structures/{structure_id}/` -- authenticated (docking
    /// access + `esi-markets.structure_markets.v1`), returns every type
    /// currently on the market in that structure (unlike
    /// `regional_market_orders`, there is no `type_id` filter), paginated
    /// the same way. `solar_system_id` is threaded through to
    /// `parse_structure_market_order` since structure order JSON has no
    /// per-order `system_id` field.
    async fn structure_market_orders(
        &self,
        access_token: &str,
        structure_id: i64,
        solar_system_id: i64,
        page: u32,
        etag: Option<&str>,
    ) -> Result<EsiResponse<MarketOrderObservation>, EsiError> {
        let _ = (access_token, structure_id, solar_system_id, page, etag);
        Err(EsiError::PermanentFailure)
    }
    async fn character_public_info(
        &self,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterPublicInfo>, EsiError> {
        let _ = character_id;
        Err(EsiError::PermanentFailure)
    }
    async fn character_location(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterLocationObservation>, EsiError> {
        let _ = (access_token, character_id);
        Err(EsiError::PermanentFailure)
    }
    async fn character_skills(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterSkillsObservation>, EsiError> {
        let _ = (access_token, character_id);
        Err(EsiError::PermanentFailure)
    }
    async fn character_skill_queue(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterSkillQueueEntry>, EsiError> {
        let _ = (access_token, character_id);
        Err(EsiError::PermanentFailure)
    }
    async fn character_industry_jobs(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterIndustryJobObservation>, EsiError> {
        let _ = (access_token, character_id);
        Err(EsiError::PermanentFailure)
    }
    async fn character_planets(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterPlanetObservation>, EsiError> {
        let _ = (access_token, character_id);
        Err(EsiError::PermanentFailure)
    }
    /// Always exactly one record on success.
    async fn character_planet_detail(
        &self,
        access_token: &str,
        character_id: i64,
        planet_id: i64,
    ) -> Result<EsiResponse<CharacterPlanetDetailObservation>, EsiError> {
        let _ = (access_token, character_id, planet_id);
        Err(EsiError::PermanentFailure)
    }
}

#[cfg(test)]
mod tests;
