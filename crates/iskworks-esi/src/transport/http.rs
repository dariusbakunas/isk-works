use super::*;

/// Bound on the connect phase (DNS + TCP + TLS) of a single ESI/SSO HTTP
/// request. A dead or unroutable host fails fast rather than pinning a
/// caller (and, for market/character work, a concurrency slot) while the
/// OS works through its own connect timeout.
pub(super) const ESI_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Total deadline for a single ESI/SSO HTTP request -- from the start of
/// connecting until the response body has finished. This is the
/// normal-operation reliability bound; cooperative cancellation (the
/// `CancellationToken` the worker races these futures against) remains the
/// shutdown mechanism. Applied per HTTP request, i.e. per page, never to a
/// whole multi-page logical operation.
pub(super) const ESI_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The one `reqwest::Client` configuration every `HttpEsiTransport` uses --
/// public ESI, authenticated ESI, OAuth token calls, and JWKS all get the
/// identical connect/request bounds. A builder failure here means the TLS
/// backend could not initialise, which is a process-wide invariant
/// violation, not a runtime condition: fail loudly rather than fall back to
/// an unbounded `Client::new()` and silently defeat this module's purpose.
/// Every request also carries the app's `User-Agent` and the pinned ESI
/// compatibility date (see `client_identity`).
pub(super) fn build_client() -> Client {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        "x-compatibility-date",
        header::HeaderValue::from_static(COMPATIBILITY_DATE),
    );
    Client::builder()
        .user_agent(process_user_agent())
        .default_headers(headers)
        .connect_timeout(ESI_CONNECT_TIMEOUT)
        .timeout(ESI_REQUEST_TIMEOUT)
        .build()
        .expect("reqwest client with ESI timeout bounds must build")
}

/// Classifies a `reqwest::Error` raised while consuming a response body.
/// A timeout (the total request deadline elapsing mid-body-read) is a
/// transient condition and must reuse the retryable `TemporaryFailure`
/// contract, exactly like a timeout raised from `send()`. Any other body
/// failure -- a malformed or truncated-but-complete JSON payload -- stays
/// `InvalidResponse`.
pub(super) fn decode_body_error(error: &reqwest::Error) -> EsiError {
    if error.is_timeout() {
        EsiError::TemporaryFailure
    } else {
        EsiError::InvalidResponse
    }
}

#[derive(Clone)]
pub struct HttpEsiTransport {
    client: Client,
    client_id: String,
    redirect_uri: String,
    token_url: String,
    jwks_url: String,
    esi_base_url: String,
    issuer: String,
    /// Process-wide unless a test swaps in its own; see `error_limit`.
    error_limit: Arc<ErrorLimitGuard>,
    /// Process-wide in production; see `rate_limit`.
    rate_limit: Arc<RateLimitGuard>,
    /// EVE SSO's signing keys, shared by clones of this transport.
    jwks: Arc<Mutex<Option<CachedJwks>>>,
    /// Pauses requests around Tranquility's daily downtime. Only for the
    /// real ESI host; a fake or proxy base URL has no downtime to wait out.
    downtime: Option<Arc<DowntimeGuard>>,
    /// The clock `downtime` is checked against; tests pin it.
    now: fn() -> DateTime<Utc>,
}

/// The host whose daily downtime `DowntimeGuard` waits out.
pub(super) const TRANQUILITY_ESI_HOST: &str = "esi.evetech.net";

pub(super) fn downtime_guard_for(esi_base_url: &str) -> Option<Arc<DowntimeGuard>> {
    let host = url::Url::parse(esi_base_url)
        .ok()?
        .host_str()?
        .to_ascii_lowercase();
    (host == TRANQUILITY_ESI_HOST).then(DowntimeGuard::global)
}

/// How long fetched SSO signing keys are trusted before refetching. EVE
/// rotates them rarely; an unknown `kid` refetches sooner (see
/// `JWKS_MIN_REFETCH`).
pub(super) const JWKS_TTL: std::time::Duration = std::time::Duration::from_secs(60 * 60);
/// A token signed with a key we don't know refetches the keys -- but never
/// more often than this, so unknown key ids can't make us hammer SSO.
pub(super) const JWKS_MIN_REFETCH: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Clone)]
pub(super) struct CachedJwks {
    keys: JwkSet,
    fetched_at: Instant,
}

impl HttpEsiTransport {
    #[must_use]
    pub fn public(esi_base_url: String) -> Self {
        Self {
            client: build_client(),
            client_id: String::new(),
            redirect_uri: String::new(),
            token_url: String::new(),
            jwks_url: String::new(),
            downtime: downtime_guard_for(&esi_base_url),
            now: Utc::now,
            esi_base_url,
            issuer: String::new(),
            error_limit: ErrorLimitGuard::global(),
            rate_limit: RateLimitGuard::global(),
            jwks: Arc::default(),
        }
    }

    #[must_use]
    pub fn new(
        client_id: String,
        redirect_uri: String,
        token_url: String,
        jwks_url: String,
        esi_base_url: String,
        issuer: String,
    ) -> Self {
        Self {
            client: build_client(),
            client_id,
            redirect_uri,
            token_url,
            jwks_url,
            downtime: downtime_guard_for(&esi_base_url),
            now: Utc::now,
            esi_base_url,
            issuer,
            error_limit: ErrorLimitGuard::global(),
            rate_limit: RateLimitGuard::global(),
            jwks: Arc::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_downtime_guard(
        mut self,
        guard: Arc<DowntimeGuard>,
        now: fn() -> DateTime<Utc>,
    ) -> Self {
        self.downtime = Some(guard);
        self.now = now;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_error_limit_guard(mut self, guard: Arc<ErrorLimitGuard>) -> Self {
        self.error_limit = guard;
        self
    }

    async fn token_request(&self, form: &[(&str, &str)]) -> Result<TokenResponse, EsiError> {
        let response = self
            .client
            .post(&self.token_url)
            .form(form)
            .send()
            .await
            .map_err(|_| EsiError::TemporaryFailure)?;
        let status = response.status();
        if status == StatusCode::BAD_REQUEST {
            // OAuth reports a revoked/expired refresh token as 400
            // `invalid_grant` -- only that one means "the user must
            // reconnect". Other 400s (`invalid_client`, ...) are ours to fix.
            let body: Option<TokenErrorResponse> = response.json().await.ok();
            return Err(match body {
                Some(body) if body.error == "invalid_grant" => EsiError::AuthorizationRequired,
                _ => EsiError::PermanentFailure,
            });
        }
        if !status.is_success() {
            return Err(classify_status(status, &response));
        }
        response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))
    }

    async fn validate_identity(&self, token: &str) -> Result<Identity, EsiError> {
        self.validate_token(token)
            .await
            .map(|(identity, _)| identity)
    }

    /// Validates the access-token JWT and returns its identity plus the
    /// `owner` claim (see `AuthenticatedToken::owner_hash`).
    pub(super) async fn validate_token(
        &self,
        token: &str,
    ) -> Result<(Identity, Option<String>), EsiError> {
        let header = decode_header(token).map_err(|_| EsiError::InvalidIdentity)?;
        if header.alg != Algorithm::RS256 {
            return Err(EsiError::InvalidIdentity);
        }
        let key_id = header.kid.ok_or(EsiError::InvalidIdentity)?;
        let keys = self.signing_keys(&key_id).await?;
        let jwk = keys.find(&key_id).ok_or(EsiError::InvalidIdentity)?;
        let key = DecodingKey::from_jwk(jwk).map_err(|_| EsiError::InvalidIdentity)?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.client_id.as_str()]);
        let claims = decode::<Claims>(token, &key, &validation)
            .map_err(|_| EsiError::InvalidIdentity)?
            .claims;
        if !claims.aud.contains(&self.client_id) || !claims.aud.contains("EVE Online") {
            return Err(EsiError::InvalidIdentity);
        }
        let character_id = claims
            .sub
            .strip_prefix("CHARACTER:EVE:")
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
            .ok_or(EsiError::InvalidIdentity)?;
        let owner_hash = claims.owner.filter(|owner| !owner.is_empty());
        Ok((
            Identity {
                character_id,
                character_name: claims.name,
                scopes: claims.scp.into_set(),
            },
            owner_hash,
        ))
    }

    /// SSO's signing keys, from cache while they are fresh and know
    /// `key_id`; otherwise refetched (at most once per `JWKS_MIN_REFETCH`).
    async fn signing_keys(&self, key_id: &str) -> Result<JwkSet, EsiError> {
        let cached = self.jwks.lock().expect("jwks lock").clone();
        if let Some(cached) = cached {
            let age = cached.fetched_at.elapsed();
            if (age < JWKS_TTL && cached.keys.find(key_id).is_some()) || age < JWKS_MIN_REFETCH {
                return Ok(cached.keys);
            }
        }
        let keys: JwkSet = self
            .client
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|_| EsiError::TemporaryFailure)?
            .json()
            .await
            .map_err(|_| EsiError::InvalidIdentity)?;
        *self.jwks.lock().expect("jwks lock") = Some(CachedJwks {
            keys: keys.clone(),
            fetched_at: Instant::now(),
        });
        Ok(keys)
    }

    /// Gate every ESI request: the error budget must not be nearly spent,
    /// and Tranquility must not be in its daily downtime.
    async fn before_request(&self, url: &str, caller: Caller) -> Result<(), EsiError> {
        self.error_limit.check()?;
        self.rate_limit.check(url, caller)?;
        self.downtime_gate().await
    }

    /// Feeds every ESI response to the error-limit and rate-limit guards.
    fn observe_response(&self, url: &str, caller: Caller, response: &reqwest::Response) {
        self.error_limit
            .observe(response.status(), response.headers());
        self.rate_limit
            .observe(url, caller, response.status(), response.headers());
    }

    /// Whether ESI is paused for Tranquility's daily downtime, for showing
    /// users. Outside the downtime window this makes no request; inside it,
    /// it shares the guard's single `/status/` probe, so asking often costs
    /// ESI nothing extra.
    pub async fn availability(&self) -> EsiAvailability {
        match self.downtime_gate().await {
            Err(EsiError::ServerDowntime {
                retry_after_seconds,
            }) => EsiAvailability {
                downtime: true,
                retry_after_seconds,
            },
            _ => EsiAvailability::default(),
        }
    }

    async fn downtime_gate(&self) -> Result<(), EsiError> {
        let Some(downtime) = &self.downtime else {
            return Ok(());
        };
        match downtime.check((self.now)()) {
            DowntimeDecision::Send => Ok(()),
            DowntimeDecision::Wait {
                retry_after_seconds,
            } => Err(EsiError::ServerDowntime {
                retry_after_seconds: Some(retry_after_seconds),
            }),
            DowntimeDecision::Probe => {
                let healthy = match self
                    .client
                    .get(format!("{}/latest/status/", self.esi_base_url))
                    .send()
                    .await
                {
                    Ok(response) => {
                        self.error_limit
                            .observe(response.status(), response.headers());
                        response.status().is_success()
                            && response.json::<ServerStatus>().await.is_ok_and(|status| {
                                restarted_since_downtime(
                                    (self.now)(),
                                    status.start_time,
                                    status.vip.unwrap_or(false),
                                )
                            })
                    }
                    Err(_) => false,
                };
                downtime.probed((self.now)(), healthy);
                if healthy {
                    Ok(())
                } else {
                    Err(EsiError::ServerDowntime {
                        retry_after_seconds: Some(30),
                    })
                }
            }
        }
    }

    async fn get_json(
        &self,
        url: String,
        access_token: &str,
        etag: Option<&str>,
    ) -> Result<reqwest::Response, EsiError> {
        let caller = rate_limit::caller(Some(access_token));
        self.before_request(&url, caller).await?;
        let mut request = self.client.get(&url).bearer_auth(access_token);
        if let Some(etag) = etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        let response = request
            .send()
            .await
            .map_err(|_| EsiError::TemporaryFailure)?;
        self.observe_response(&url, caller, &response);
        if response.status() == StatusCode::NOT_MODIFIED || response.status().is_success() {
            Ok(response)
        } else {
            Err(classify_status(response.status(), &response))
        }
    }

    async fn get_public_json(&self, url: String) -> Result<reqwest::Response, EsiError> {
        self.before_request(&url, None).await?;
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|_| EsiError::TemporaryFailure)?;
        self.observe_response(&url, None, &response);
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(classify_status(response.status(), &response))
        }
    }

    async fn post_public_json<T: Serialize + ?Sized>(
        &self,
        url: String,
        body: &T,
    ) -> Result<reqwest::Response, EsiError> {
        self.before_request(&url, None).await?;
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|_| EsiError::TemporaryFailure)?;
        self.observe_response(&url, None, &response);
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(classify_status(response.status(), &response))
        }
    }
}

#[async_trait]
impl EsiTransport for HttpEsiTransport {
    async fn exchange_code(
        &self,
        code: &str,
        verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        let response = self
            .token_request(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &self.client_id),
                ("code_verifier", verifier.expose()),
                ("redirect_uri", &self.redirect_uri),
            ])
            .await?;
        let refresh_token = response.refresh_token.ok_or(EsiError::InvalidResponse)?;
        let (identity, owner_hash) = self.validate_token(&response.access_token).await?;
        Ok(AuthenticatedToken {
            access_token: response.access_token,
            refresh_token,
            expires_at: Utc::now() + Duration::seconds(response.expires_in),
            identity,
            owner_hash,
        })
    }

    async fn refresh(&self, refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        let response = self
            .token_request(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", &self.client_id),
            ])
            .await?;
        let identity = self.validate_identity(&response.access_token).await?;
        Ok(RefreshedToken {
            access_token: response.access_token,
            rotated_refresh_token: response.refresh_token,
            expires_at: Utc::now() + Duration::seconds(response.expires_in),
            identity,
        })
    }

    async fn revoke_refresh_token(&self, refresh_token: &str) -> Result<(), EsiError> {
        // EVE serves revocation next to the token endpoint
        // (`.../v2/oauth/token` -> `.../v2/oauth/revoke`).
        let revoke_url = match self.token_url.strip_suffix("/token") {
            Some(base) => format!("{base}/revoke"),
            None => return Err(EsiError::Configuration("token URL has no /token suffix")),
        };
        let response = self
            .client
            .post(revoke_url)
            .form(&[
                ("token_type_hint", "refresh_token"),
                ("token", refresh_token),
                ("client_id", &self.client_id),
            ])
            .send()
            .await
            .map_err(|_| EsiError::TemporaryFailure)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(classify_status(response.status(), &response))
        }
    }

    async fn assets(
        &self,
        access_token: &str,
        character_id: i64,
        page: u32,
        etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/assets/?page={page}",
                    self.esi_base_url
                ),
                access_token,
                etag,
            )
            .await?;
        let metadata = metadata(&response);
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(EsiResponse {
                records: Vec::new(),
                not_modified: true,
                metadata,
            });
        }
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        let records = values
            .into_iter()
            .map(parse_asset)
            .collect::<Result<_, _>>()?;
        Ok(EsiResponse {
            records,
            not_modified: false,
            metadata,
        })
    }

    async fn blueprints(
        &self,
        access_token: &str,
        character_id: i64,
        page: u32,
    ) -> Result<EsiResponse<BlueprintAssetObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/blueprints/?page={page}",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let metadata = metadata(&response);
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(EsiResponse {
            records: values
                .into_iter()
                .map(parse_blueprint)
                .collect::<Result<_, _>>()?,
            not_modified: false,
            metadata,
        })
    }

    async fn wallet_transactions(
        &self,
        access_token: &str,
        character_id: i64,
        from_id: Option<i64>,
        etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        let mut url = format!(
            "{}/latest/characters/{character_id}/wallet/transactions/",
            self.esi_base_url
        );
        if let Some(from_id) = from_id {
            url.push_str(&format!("?from_id={from_id}"));
        }
        let response = self.get_json(url, access_token, etag).await?;
        let metadata = metadata(&response);
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(EsiResponse {
                records: Vec::new(),
                not_modified: true,
                metadata,
            });
        }
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        let records = values
            .into_iter()
            .map(parse_wallet)
            .collect::<Result<_, _>>()?;
        Ok(EsiResponse {
            records,
            not_modified: false,
            metadata,
        })
    }

    async fn wallet_balance(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<WalletBalanceObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/wallet/",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let metadata = metadata(&response);
        let raw: Value = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(EsiResponse {
            records: vec![parse_wallet_balance(raw)?],
            not_modified: false,
            metadata,
        })
    }

    async fn wallet_journal(
        &self,
        access_token: &str,
        character_id: i64,
        page: u32,
    ) -> Result<EsiResponse<WalletJournalObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/wallet/journal/?page={page}",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let metadata = metadata(&response);
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(EsiResponse {
            records: values
                .into_iter()
                .map(parse_wallet_journal)
                .collect::<Result<_, _>>()?,
            not_modified: false,
            metadata,
        })
    }

    async fn universe_names(&self, ids: &[i64]) -> Result<Vec<EveEntityName>, EsiError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let response = self
            .post_public_json(format!("{}/latest/universe/names/", self.esi_base_url), ids)
            .await?;
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        values.into_iter().map(parse_entity_name).collect()
    }

    async fn structure(
        &self,
        access_token: &str,
        structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/universe/structures/{structure_id}/",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let raw: Value = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(StructureInformation {
            structure_id,
            name: required_str(&raw, "name")?,
            owner_id: required_i64(&raw, "owner_id")?,
            solar_system_id: required_i64(&raw, "solar_system_id")?,
            type_id: raw.get("type_id").and_then(Value::as_i64),
        })
    }

    async fn industry_systems(&self) -> Result<EsiResponse<IndustrySystemCostIndex>, EsiError> {
        let response = self
            .get_public_json(format!("{}/latest/industry/systems/", self.esi_base_url))
            .await?;
        let metadata = metadata(&response);
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        let records = values
            .into_iter()
            .map(parse_industry_system)
            .collect::<Result<_, _>>()?;
        Ok(EsiResponse {
            records,
            not_modified: false,
            metadata,
        })
    }

    async fn market_prices(&self) -> Result<EsiResponse<AdjustedPrice>, EsiError> {
        let response = self
            .get_public_json(format!("{}/latest/markets/prices/", self.esi_base_url))
            .await?;
        let metadata = metadata(&response);
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        let records = values
            .into_iter()
            .filter(|value| value.get("adjusted_price").is_some())
            .map(parse_adjusted_price)
            .collect::<Result<_, _>>()?;
        Ok(EsiResponse {
            records,
            not_modified: false,
            metadata,
        })
    }

    async fn regional_market_orders(
        &self,
        region_id: i64,
        type_id: i64,
        page: u32,
        etag: Option<&str>,
    ) -> Result<EsiResponse<MarketOrderObservation>, EsiError> {
        let url = format!(
            "{}/latest/markets/{region_id}/orders/?order_type=all&page={page}&type_id={type_id}",
            self.esi_base_url
        );
        self.before_request(&url, None).await?;
        let mut request = self.client.get(&url);
        if let Some(etag) = etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(|error| {
            tracing::warn!(
                endpoint = "regional_market_orders",
                region_id,
                type_id,
                page,
                category = request_failure_category(&error),
                cause = request_failure_cause(&error),
                is_timeout = error.is_timeout(),
                is_connect = error.is_connect(),
                is_request = error.is_request(),
                is_body = error.is_body(),
                is_decode = error.is_decode(),
                is_builder = error.is_builder(),
                is_redirect = error.is_redirect(),
                is_status = error.is_status(),
                "esi market request failed"
            );
            classify_request_error(&error)
        })?;
        self.observe_response(&url, None, &response);
        if response.status() != StatusCode::NOT_MODIFIED && !response.status().is_success() {
            tracing::warn!(
                endpoint = "regional_market_orders",
                region_id,
                type_id,
                page,
                status = response.status().as_u16(),
                category = http_failure_category(response.status()),
                retry_after_seconds = header_u64(&response, header::RETRY_AFTER),
                error_limit_remain = header_u64(&response, "x-esi-error-limit-remain"),
                error_limit_reset = header_u64(&response, "x-esi-error-limit-reset"),
                "esi market request failed"
            );
            return Err(classify_status(response.status(), &response));
        }
        let status = response.status();
        let metadata = metadata(&response);
        if response.status() == StatusCode::NOT_MODIFIED {
            tracing::debug!(
                endpoint = "regional_market_orders",
                region_id,
                type_id,
                page,
                status = status.as_u16(),
                not_modified = true,
                record_count = 0,
                page_count = metadata.pages.unwrap_or(1),
                error_limit_remain = metadata.error_limit_remain,
                error_limit_reset = metadata.error_limit_reset,
                "esi market request completed"
            );
            return Ok(EsiResponse {
                records: Vec::new(),
                not_modified: true,
                metadata,
            });
        }
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        tracing::debug!(
            endpoint = "regional_market_orders",
            region_id,
            type_id,
            page,
            status = status.as_u16(),
            not_modified = false,
            record_count = values.len(),
            page_count = metadata.pages.unwrap_or(1),
            error_limit_remain = metadata.error_limit_remain,
            error_limit_reset = metadata.error_limit_reset,
            "esi market request completed"
        );
        Ok(EsiResponse {
            records: values
                .into_iter()
                .map(parse_market_order)
                .collect::<Result<_, _>>()?,
            not_modified: false,
            metadata,
        })
    }

    async fn structure_market_orders(
        &self,
        access_token: &str,
        structure_id: i64,
        solar_system_id: i64,
        page: u32,
        etag: Option<&str>,
    ) -> Result<EsiResponse<MarketOrderObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/markets/structures/{structure_id}/?page={page}",
                    self.esi_base_url
                ),
                access_token,
                etag,
            )
            .await?;
        let status = response.status();
        let metadata = metadata(&response);
        if status == StatusCode::NOT_MODIFIED {
            tracing::debug!(
                endpoint = "structure_market_orders",
                structure_id,
                page,
                status = status.as_u16(),
                not_modified = true,
                record_count = 0,
                page_count = metadata.pages.unwrap_or(1),
                error_limit_remain = metadata.error_limit_remain,
                error_limit_reset = metadata.error_limit_reset,
                "esi structure market request completed"
            );
            return Ok(EsiResponse {
                records: Vec::new(),
                not_modified: true,
                metadata,
            });
        }
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        tracing::debug!(
            endpoint = "structure_market_orders",
            structure_id,
            page,
            status = status.as_u16(),
            not_modified = false,
            record_count = values.len(),
            page_count = metadata.pages.unwrap_or(1),
            error_limit_remain = metadata.error_limit_remain,
            error_limit_reset = metadata.error_limit_reset,
            "esi structure market request completed"
        );
        Ok(EsiResponse {
            records: values
                .into_iter()
                .map(|raw| parse_structure_market_order(raw, solar_system_id))
                .collect::<Result<_, _>>()?,
            not_modified: false,
            metadata,
        })
    }

    async fn character_public_info(
        &self,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterPublicInfo>, EsiError> {
        let response = self
            .get_public_json(format!(
                "{}/latest/characters/{character_id}/",
                self.esi_base_url
            ))
            .await?;
        let metadata = metadata(&response);
        let raw: Value = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(EsiResponse {
            records: vec![parse_character_public_info(character_id, raw)?],
            not_modified: false,
            metadata,
        })
    }

    async fn character_location(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterLocationObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/location/",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let metadata = metadata(&response);
        let raw: Value = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(EsiResponse {
            records: vec![parse_character_location(raw)?],
            not_modified: false,
            metadata,
        })
    }

    async fn character_skills(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterSkillsObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/skills/",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let metadata = metadata(&response);
        let raw: Value = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(EsiResponse {
            records: vec![parse_character_skills(raw)?],
            not_modified: false,
            metadata,
        })
    }

    async fn character_skill_queue(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterSkillQueueEntry>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/skillqueue/",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let metadata = metadata(&response);
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        let entries = values
            .into_iter()
            .map(parse_character_skill_queue_entry)
            .collect::<Result<_, _>>()?;
        Ok(EsiResponse {
            records: sort_skill_queue_entries(entries),
            not_modified: false,
            metadata,
        })
    }

    async fn character_industry_jobs(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterIndustryJobObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/industry/jobs/",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let metadata = metadata(&response);
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(EsiResponse {
            records: values
                .into_iter()
                .map(parse_character_industry_job)
                .collect::<Result<_, _>>()?,
            not_modified: false,
            metadata,
        })
    }

    async fn character_planets(
        &self,
        access_token: &str,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterPlanetObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/planets/",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let metadata = metadata(&response);
        let values: Vec<Value> = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(EsiResponse {
            records: values
                .into_iter()
                .map(parse_character_planet)
                .collect::<Result<_, _>>()?,
            not_modified: false,
            metadata,
        })
    }

    async fn character_planet_detail(
        &self,
        access_token: &str,
        character_id: i64,
        planet_id: i64,
    ) -> Result<EsiResponse<CharacterPlanetDetailObservation>, EsiError> {
        let response = self
            .get_json(
                format!(
                    "{}/latest/characters/{character_id}/planets/{planet_id}/",
                    self.esi_base_url
                ),
                access_token,
                None,
            )
            .await?;
        let metadata = metadata(&response);
        let value: Value = response
            .json()
            .await
            .map_err(|error| decode_body_error(&error))?;
        Ok(EsiResponse {
            records: vec![parse_character_planet_detail(&value)?],
            not_modified: false,
            metadata,
        })
    }
}

/// The part of ESI's `/status/` answer the downtime probe reads.
#[derive(Deserialize)]
struct ServerStatus {
    start_time: DateTime<Utc>,
    vip: Option<bool>,
}
