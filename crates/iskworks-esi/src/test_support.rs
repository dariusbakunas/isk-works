//! Helpers for tests in this and other workspace crates. Compiled only for
//! this crate's tests or with the `test-support` feature.

use async_trait::async_trait;

use crate::{
    AssetObservation, AuthenticatedToken, EsiError, EsiResponse, EsiTransport,
    IndustrySystemCostIndex, PkceVerifier, RefreshedToken, SecretCipher, StructureInformation,
    WalletTransactionObservation,
};

impl SecretCipher {
    /// A fixed, publicly known key. Its base64 decodes to the 32 ASCII bytes
    /// `test-only-key-do-not-use-in-prod`.
    #[doc(hidden)]
    #[must_use]
    pub fn for_tests() -> Self {
        Self::from_base64("dGVzdC1vbmx5LWtleS1kby1ub3QtdXNlLWluLXByb2Q=")
            .expect("the test key is valid base64 of 32 bytes")
    }
}

/// A transport for tests that never reach ESI: every call panics.
#[doc(hidden)]
pub struct UnusedEsiTransport;

#[async_trait]
impl EsiTransport for UnusedEsiTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        unimplemented!("these tests never exchange an OAuth code")
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        unimplemented!("these tests never refresh a token")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        unimplemented!("these tests never read ESI resources")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!("these tests never read ESI resources")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!("these tests never read ESI resources")
    }

    async fn industry_systems(&self) -> Result<EsiResponse<IndustrySystemCostIndex>, EsiError> {
        unimplemented!("these tests never read ESI resources")
    }
}
