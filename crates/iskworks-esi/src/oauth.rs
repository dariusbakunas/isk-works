use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use url::Url;

use crate::EsiError;

pub const ASSET_SCOPE: &str = "esi-assets.read_assets.v1";
pub const BLUEPRINT_SCOPE: &str = "esi-characters.read_blueprints.v1";
pub const STRUCTURE_SCOPE: &str = "esi-universe.read_structures.v1";
pub const WALLET_SCOPE: &str = "esi-wallet.read_character_wallet.v1";
pub const LOCATION_SCOPE: &str = "esi-location.read_location.v1";
pub const SKILLS_SCOPE: &str = "esi-skills.read_skills.v1";
pub const SKILL_QUEUE_SCOPE: &str = "esi-skills.read_skillqueue.v1";
pub const INDUSTRY_JOBS_SCOPE: &str = "esi-industry.read_character_jobs.v1";
pub const MARKET_STRUCTURE_SCOPE: &str = "esi-markets.structure_markets.v1";
/// Read access to the character's PI colonies (ESI names it "manage", but
/// the endpoints ISK Works calls are read-only).
pub const PLANETS_SCOPE: &str = "esi-planets.manage_planets.v1";

#[derive(Clone, Eq, PartialEq)]
pub struct OAuthState(String);

impl OAuthState {
    #[must_use]
    pub fn generate() -> Self {
        Self(random_value())
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for OAuthState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OAuthState([redacted])")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PkceVerifier(String);

impl PkceVerifier {
    pub fn from_secret(value: String) -> Result<Self, EsiError> {
        if !(43..=128).contains(&value.len())
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
            })
        {
            return Err(EsiError::SecretDecryption);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for PkceVerifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PkceVerifier([redacted])")
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PkceChallenge(String);

impl PkceChallenge {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[must_use]
pub fn new_pkce() -> (PkceVerifier, PkceChallenge) {
    let verifier = PkceVerifier(random_value());
    let challenge = PkceChallenge(URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.0.as_bytes())));
    (verifier, challenge)
}

#[must_use]
pub fn hash_state(state: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(state.as_bytes()))
}

pub fn authorization_url(
    base: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    state: &OAuthState,
    challenge: &PkceChallenge,
) -> Result<String, EsiError> {
    let mut url =
        Url::parse(base).map_err(|_| EsiError::Configuration("invalid authorization URL"))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", &scopes.join(" "))
        .append_pair("state", state.expose())
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", challenge.as_str());
    Ok(url.into())
}

fn random_value() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_and_state_are_url_safe_and_non_repeating() {
        let (first, challenge) = new_pkce();
        let (second, _) = new_pkce();
        assert_eq!(first.expose().len(), 43);
        assert_ne!(first, second);
        assert_eq!(challenge.as_str().len(), 43);
        assert_ne!(hash_state("first"), hash_state("second"));
    }

    #[test]
    fn authorization_url_contains_only_protocol_inputs() {
        let (verifier, challenge) = new_pkce();
        let state = OAuthState::generate();
        let value = authorization_url(
            "https://login.eveonline.com/v2/oauth/authorize",
            "client",
            "https://example.test/callback",
            &[
                ASSET_SCOPE.to_string(),
                BLUEPRINT_SCOPE.to_string(),
                WALLET_SCOPE.to_string(),
                STRUCTURE_SCOPE.to_string(),
            ],
            &state,
            &challenge,
        )
        .unwrap();
        assert!(value.contains("code_challenge_method=S256"));
        assert!(!value.contains(verifier.expose()));
    }
}
