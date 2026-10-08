use super::*;

impl EsiApplicationService {
    pub async fn system_cost_index(
        &self,
        solar_system_id: i64,
    ) -> Result<SystemCostIndex, EsiApplicationError> {
        if solar_system_id <= 0 {
            return Err(InventoryError::Validation(
                "Solar system ID must be positive.".to_string(),
            )
            .into());
        }
        let now = Utc::now();
        if let Some(cached) = self.industry_index_cache.read().await.as_ref() {
            if cached.expires_at > now {
                return cost_index_from_cache(cached, solar_system_id);
            }
        }

        let response = self.transport.industry_systems().await?;
        let fetched_at = Utc::now();
        let expires_at = response
            .metadata
            .expires
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc2822(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .filter(|value| *value > fetched_at)
            .unwrap_or_else(|| fetched_at + Duration::hours(1));
        let cache = IndustryIndexCache {
            values: response
                .records
                .into_iter()
                .map(|system| (system.solar_system_id, system))
                .collect(),
            fetched_at,
            expires_at,
        };
        let result = cost_index_from_cache(&cache, solar_system_id);
        *self.industry_index_cache.write().await = Some(cache);
        result
    }

    pub async fn automatic_eiv(
        &self,
        materials: &[CapturedRecipeLine],
        runs: u64,
    ) -> Result<AutomaticEiv, EsiApplicationError> {
        let now = Utc::now();
        if let Some(cached) = self.adjusted_price_cache.read().await.as_ref() {
            if cached.expires_at > now {
                return eiv_from_cache(cached, materials, runs);
            }
        }

        let type_ids = materials
            .iter()
            .map(|line| line.type_id)
            .collect::<Vec<_>>();
        let stored = self
            .repository
            .latest_adjusted_prices(&type_ids, now)
            .await?;
        if type_ids.iter().all(|type_id| stored.contains_key(type_id)) {
            let value = calculate_adjusted_price_eiv(materials, runs, &stored)?;
            return Ok(AutomaticEiv {
                value: value.value,
                missing_type_ids: value.missing_type_ids,
                observed_at: now,
                expires_at: now + Duration::hours(1),
            });
        }

        let response = self.transport.market_prices().await?;
        let fetched_at = Utc::now();
        let expires_at = response
            .metadata
            .expires
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc2822(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .filter(|value| *value > fetched_at)
            .unwrap_or_else(|| fetched_at + Duration::hours(1));
        self.repository
            .save_adjusted_prices(
                &response.records,
                fetched_at,
                expires_at,
                response.metadata.etag.as_deref(),
            )
            .await?;
        let cache = AdjustedPriceCache {
            values: response
                .records
                .into_iter()
                .map(|price| (price.type_id, price.adjusted_price))
                .collect(),
            fetched_at,
            expires_at,
        };
        let result = eiv_from_cache(&cache, materials, runs);
        *self.adjusted_price_cache.write().await = Some(cache);
        result
    }

    /// Resolve adjusted prices for `type_ids` **once, in bulk** -- the union of
    /// every walked operation's base recipe material types. Shares the exact
    /// cache / stored-then-fetch resolution [`Self::automatic_eiv`] uses:
    /// serve from the in-memory cache while fresh, else the repository, else a
    /// single ESI `/markets/prices` fetch (persisted + cached). A type with no
    /// adjusted price anywhere is returned in `missing_type_ids` -- never
    /// substituted. Empty input short-circuits with no I/O.
    pub async fn adjusted_prices(
        &self,
        type_ids: &[i64],
    ) -> Result<AdjustedPriceSet, EsiApplicationError> {
        let now = Utc::now();
        if type_ids.is_empty() {
            return Ok(AdjustedPriceSet {
                values: std::collections::BTreeMap::new(),
                missing_type_ids: Vec::new(),
                observed_at: now,
            });
        }

        let mut wanted: Vec<i64> = type_ids.to_vec();
        wanted.sort_unstable();
        wanted.dedup();

        if let Some(cached) = self.adjusted_price_cache.read().await.as_ref() {
            if cached.expires_at > now {
                return Ok(subset(&cached.values, &wanted, cached.fetched_at));
            }
        }

        let stored = self.repository.latest_adjusted_prices(&wanted, now).await?;
        if wanted.iter().all(|type_id| stored.contains_key(type_id)) {
            return Ok(subset(&stored, &wanted, now));
        }

        let response = self.transport.market_prices().await?;
        let fetched_at = Utc::now();
        let expires_at = response
            .metadata
            .expires
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc2822(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .filter(|value| *value > fetched_at)
            .unwrap_or_else(|| fetched_at + Duration::hours(1));
        self.repository
            .save_adjusted_prices(
                &response.records,
                fetched_at,
                expires_at,
                response.metadata.etag.as_deref(),
            )
            .await?;
        let cache = AdjustedPriceCache {
            values: response
                .records
                .into_iter()
                .map(|price| (price.type_id, price.adjusted_price))
                .collect(),
            fetched_at,
            expires_at,
        };
        let result = subset(&cache.values, &wanted, fetched_at);
        *self.adjusted_price_cache.write().await = Some(cache);
        Ok(result)
    }
}

pub(super) fn cost_index_from_cache(
    cache: &IndustryIndexCache,
    solar_system_id: i64,
) -> Result<SystemCostIndex, EsiApplicationError> {
    let entry = cache.values.get(&solar_system_id).ok_or_else(|| {
        InventoryError::Validation(
            "ESI did not return a cost index for this solar system.".to_string(),
        )
    })?;
    Ok(SystemCostIndex {
        solar_system_id,
        manufacturing: entry.manufacturing.to_string(),
        reaction: entry.reaction.to_string(),
        fetched_at: cache.fetched_at,
        expires_at: cache.expires_at,
    })
}

/// Narrow a full `type_id -> adjusted_price` map to just `wanted`, recording
/// the ones that are absent.
pub(super) fn subset(
    all: &std::collections::BTreeMap<i64, Decimal>,
    wanted: &[i64],
    observed_at: chrono::DateTime<Utc>,
) -> AdjustedPriceSet {
    let mut values = std::collections::BTreeMap::new();
    let mut missing_type_ids = Vec::new();
    for type_id in wanted {
        match all.get(type_id) {
            Some(price) => {
                values.insert(*type_id, *price);
            }
            None => missing_type_ids.push(*type_id),
        }
    }
    AdjustedPriceSet {
        values,
        missing_type_ids,
        observed_at,
    }
}

pub(super) fn eiv_from_cache(
    cache: &AdjustedPriceCache,
    materials: &[CapturedRecipeLine],
    runs: u64,
) -> Result<AutomaticEiv, EsiApplicationError> {
    let AdjustedPriceEiv {
        value,
        missing_type_ids,
    } = calculate_adjusted_price_eiv(materials, runs, &cache.values)?;
    Ok(AutomaticEiv {
        value,
        missing_type_ids,
        observed_at: cache.fetched_at,
        expires_at: cache.expires_at,
    })
}
