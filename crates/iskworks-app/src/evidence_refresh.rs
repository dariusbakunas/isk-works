//! Keeps the shared adjusted-price and system cost index evidence current:
//! the worker's two refresh passes, on `EvidenceRefreshRepository`'s claim
//! protocol. Demand is registered through `AdjustedPriceRepository` by the
//! features that read the evidence (Opportunities, build costing).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use iskworks_core::{AdjustedPriceRepository, EvidenceRefreshRepository};
use iskworks_esi::{EsiError, EsiTransport};

/// When evidence goes stale, how long a claim holds, how soon a failure
/// retries, and how many systems one pass claims.
#[derive(Clone, Copy, Debug)]
pub struct EvidenceRefreshSchedule {
    pub adjusted_price_freshness: Duration,
    pub system_index_freshness: Duration,
    pub lease_duration: Duration,
    /// The shortest wait after a failed refresh (see `retry_floor`).
    pub retry_delay: Duration,
    pub system_batch_size: i64,
}

impl Default for EvidenceRefreshSchedule {
    fn default() -> Self {
        Self {
            adjusted_price_freshness: Duration::from_secs(6 * 60 * 60),
            system_index_freshness: Duration::from_secs(60 * 60),
            lease_duration: Duration::from_secs(2 * 60),
            retry_delay: Duration::from_secs(60),
            system_batch_size: 100,
        }
    }
}

#[derive(Clone)]
pub struct EvidenceRefreshService {
    repository: Arc<dyn EvidenceRefreshRepository>,
    adjusted_prices: Arc<dyn AdjustedPriceRepository>,
    transport: Arc<dyn EsiTransport>,
    schedule: EvidenceRefreshSchedule,
}

impl EvidenceRefreshService {
    pub fn new<R, T>(repository: Arc<R>, transport: Arc<T>) -> Self
    where
        R: EvidenceRefreshRepository + AdjustedPriceRepository + 'static,
        T: EsiTransport + 'static,
    {
        Self {
            repository: repository.clone(),
            adjusted_prices: repository,
            transport,
            schedule: EvidenceRefreshSchedule::default(),
        }
    }

    #[must_use]
    pub fn with_schedule(mut self, schedule: EvidenceRefreshSchedule) -> Self {
        self.schedule = schedule;
        self
    }

    /// One adjusted-price pass: registers the singleton refresh row, then,
    /// if it is due and this worker wins the claim, fetches
    /// `/markets/prices/` and stores it. Returns whether it refreshed.
    pub async fn refresh_due_adjusted_prices(&self, now: DateTime<Utc>) -> Result<bool, String> {
        self.adjusted_prices
            .register_adjusted_price_refresh(now)
            .await
            .map_err(|error| error.to_string())?;
        if !self
            .repository
            .adjusted_price_refresh_due(now)
            .await
            .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        let lease_expires_at = now
            + chrono::Duration::from_std(self.schedule.lease_duration)
                .map_err(|error| error.to_string())?;
        let Some(claim) = self
            .repository
            .begin_adjusted_price_refresh(now, lease_expires_at)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(false);
        };
        match self.transport.market_prices().await {
            Ok(response) => {
                let next_refresh_at = now
                    + chrono::Duration::from_std(self.schedule.adjusted_price_freshness)
                        .map_err(|error| error.to_string())?;
                self.repository
                    .complete_adjusted_price_refresh(
                        claim,
                        &response.records,
                        now,
                        next_refresh_at,
                        response.metadata.expires_at(),
                        response.metadata.etag.as_deref(),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(true)
            }
            Err(error) => {
                let next_retry = now + retry_floor(self.schedule.retry_delay, &error);
                self.repository
                    .fail_adjusted_price_refresh(claim, now, next_retry, &error.to_string())
                    .await
                    .map_err(|storage_error| storage_error.to_string())?;
                Err(error.to_string())
            }
        }
    }

    /// One system-index pass: claims up to `system_batch_size` due
    /// systems, fetches `/industry/systems/` once for all of them, and
    /// completes or fails each claim. Returns how many systems it claimed.
    pub async fn refresh_due_system_indices(&self, now: DateTime<Utc>) -> Result<usize, String> {
        let systems = self
            .repository
            .system_cost_index_refresh_candidates(now, self.schedule.system_batch_size)
            .await
            .map_err(|error| error.to_string())?;
        let lease_expires_at = now
            + chrono::Duration::from_std(self.schedule.lease_duration)
                .map_err(|error| error.to_string())?;
        let mut claimed = Vec::new();
        let mut persistence_failures = Vec::new();
        for system_id in systems {
            match self
                .repository
                .begin_system_cost_index_refresh(system_id, now, lease_expires_at)
                .await
            {
                Ok(Some(claim)) => claimed.push((system_id, claim)),
                Ok(None) => {}
                Err(error) => persistence_failures.push(format!("{system_id}: {error}")),
            }
        }
        if claimed.is_empty() {
            return if persistence_failures.is_empty() {
                Ok(0)
            } else {
                Err(persistence_failures.join("; "))
            };
        }
        match self.transport.industry_systems().await {
            Ok(response) => {
                let values = response
                    .records
                    .into_iter()
                    .map(|value| (value.solar_system_id, value.manufacturing))
                    .collect::<BTreeMap<_, _>>();
                let next_refresh_at = now
                    + chrono::Duration::from_std(self.schedule.system_index_freshness)
                        .map_err(|error| error.to_string())?;
                for (system_id, claim) in &claimed {
                    if let Some(value) = values.get(system_id) {
                        if let Err(error) = self
                            .repository
                            .complete_system_cost_index_refresh(
                                *system_id,
                                *claim,
                                *value,
                                now,
                                next_refresh_at,
                                response.metadata.expires_at(),
                                response.metadata.etag.as_deref(),
                            )
                            .await
                        {
                            persistence_failures.push(format!("{system_id}: {error}"));
                        }
                    } else {
                        if let Err(error) = self
                            .repository
                            .fail_system_cost_index_refresh(
                                *system_id,
                                *claim,
                                now,
                                next_refresh_at,
                                "ESI response omitted the registered solar system",
                            )
                            .await
                        {
                            persistence_failures.push(format!("{system_id}: {error}"));
                        }
                    }
                }
                if persistence_failures.is_empty() {
                    Ok(claimed.len())
                } else {
                    Err(persistence_failures.join("; "))
                }
            }
            Err(error) => {
                let next_retry = now + retry_floor(self.schedule.retry_delay, &error);
                for (system_id, claim) in &claimed {
                    if let Err(storage_error) = self
                        .repository
                        .fail_system_cost_index_refresh(
                            *system_id,
                            *claim,
                            now,
                            next_retry,
                            &error.to_string(),
                        )
                        .await
                    {
                        persistence_failures.push(format!("{system_id}: {storage_error}"));
                    }
                }
                persistence_failures.push(error.to_string());
                Err(persistence_failures.join("; "))
            }
        }
    }
}

/// The earliest retry after a failed ESI refresh: the configured delay, or
/// longer when ESI asked us to wait (`EsiError::retry_after`). Storage adds
/// exponential backoff on top for repeated failures.
fn retry_floor(retry_delay: Duration, error: &EsiError) -> chrono::Duration {
    let wait = error.retry_after().unwrap_or_default().max(retry_delay);
    chrono::Duration::from_std(wait).unwrap_or_else(|_| chrono::Duration::hours(6))
}

#[cfg(test)]
mod tests;
