use super::*;

/// The player structure a structure-scope refresh fetches, and the
/// workspace coverage it fills.
#[derive(Clone, Copy)]
pub(super) struct StructureRefreshTarget {
    pub(super) workspace_id: WorkspaceId,
    pub(super) source_id: PriceSourceId,
    pub(super) scope: MarketScope,
    pub(super) location_id: i64,
    pub(super) solar_system_id: i64,
}

impl PublicMarketService {
    /// Resolves market access once and fetches every page for `location_id`
    /// in a single pass, then splits the combined order book across every
    /// claimed candidate by `type_id` -- one structure fetch serves however
    /// many types are due, rather than one fetch per type.
    /// Bounded to a single re-resolution: if a page beyond the first (which
    /// access resolution itself already fetched successfully) comes back
    /// `AccessDenied` -- e.g. the token was revoked mid-fetch -- the
    /// remembered connection is cleared and access is resolved fresh
    /// exactly once more before giving up, so a persistently broken
    /// structure can't loop forever.
    pub(super) async fn refresh_structure_scope(
        &self,
        target: StructureRefreshTarget,
        claimed: Vec<(MarketCoverageItem, DateTime<Utc>)>,
        cancel: &CancellationToken,
    ) -> Vec<MarketRefreshOutcome> {
        let StructureRefreshTarget {
            workspace_id,
            source_id,
            location_id,
            solar_system_id,
            ..
        } = target;
        if cancel.is_cancelled() {
            // Rows are leased to `refreshing`; recover via `lease_expires_at`.
            return claimed
                .iter()
                .map(|_| MarketRefreshOutcome::Skipped)
                .collect();
        }
        let Some(market_access) = self.market_access.clone() else {
            return self
                .fail_claimed(
                    workspace_id,
                    source_id,
                    &claimed,
                    "ESI is not configured; cannot fetch structure market orders".to_string(),
                )
                .await;
        };

        let mut allow_retry = true;
        loop {
            let preferred = match self
                .repository
                .market_access_connection(workspace_id, location_id)
                .await
            {
                Ok(preferred) => preferred,
                Err(error) => {
                    return self
                        .fail_claimed(workspace_id, source_id, &claimed, error.to_string())
                        .await;
                }
            };
            let resolution = match market_access
                .resolve_market_access(workspace_id, location_id, solar_system_id, preferred)
                .await
            {
                Ok(resolution) => resolution,
                Err(error) => {
                    return self
                        .fail_claimed(workspace_id, source_id, &claimed, error.to_string())
                        .await;
                }
            };
            let (connection_id, access_token, first_page) = match resolution {
                MarketAccessResolution::Confirmed {
                    connection_id,
                    access_token,
                    first_page,
                    ..
                } => (connection_id, access_token, first_page),
                MarketAccessResolution::NoEligibleCharacter => {
                    return self
                        .fail_claimed(
                            workspace_id,
                            source_id,
                            &claimed,
                            "no connected character has granted esi-markets.structure_markets.v1 access".to_string(),
                        )
                        .await;
                }
                MarketAccessResolution::AllDenied => {
                    return self
                        .fail_claimed(
                            workspace_id,
                            source_id,
                            &claimed,
                            "every connected character with market access was denied docking access to this structure".to_string(),
                        )
                        .await;
                }
            };

            if Some(connection_id) != preferred {
                if let Err(error) = self
                    .repository
                    .remember_market_access(workspace_id, location_id, connection_id, Utc::now())
                    .await
                {
                    tracing::warn!(%error, location_id, "failed to remember structure market access");
                }
            }

            match self
                .fetch_structure_pages(
                    &access_token,
                    location_id,
                    solar_system_id,
                    first_page,
                    cancel,
                )
                .await
            {
                Err(FetchStructureError::Cancelled) => {
                    return claimed
                        .iter()
                        .map(|_| MarketRefreshOutcome::Skipped)
                        .collect();
                }
                Ok(records) => {
                    return self
                        .complete_structure_batches(target, &claimed, records)
                        .await;
                }
                Err(FetchStructureError::AccessDenied) => {
                    if let Err(error) = self
                        .repository
                        .clear_market_access(workspace_id, location_id, Utc::now())
                        .await
                    {
                        tracing::warn!(%error, location_id, "failed to clear structure market access");
                    }
                    if allow_retry {
                        allow_retry = false;
                        continue;
                    }
                    return self
                        .fail_claimed(
                            workspace_id,
                            source_id,
                            &claimed,
                            "structure market access was denied while fetching orders".to_string(),
                        )
                        .await;
                }
                Err(FetchStructureError::Other(message)) => {
                    return self
                        .fail_claimed(workspace_id, source_id, &claimed, message)
                        .await;
                }
            }
        }
    }

    /// Fetches pages `2..=pages` beyond the already-fetched `first_page`
    /// (the real page 1 access resolution obtained -- never re-fetched).
    async fn fetch_structure_pages(
        &self,
        access_token: &str,
        location_id: i64,
        solar_system_id: i64,
        first_page: EsiResponse<MarketOrderObservation>,
        cancel: &CancellationToken,
    ) -> Result<Vec<MarketOrderObservation>, FetchStructureError> {
        if first_page.not_modified {
            return Err(FetchStructureError::Other(
                "EVE returned an invalid market order response".to_string(),
            ));
        }
        let pages = first_page.metadata.pages.unwrap_or(1);
        if pages == 0 || pages > MAX_PAGES_PER_TYPE {
            return Err(FetchStructureError::Other(
                "EVE returned an invalid market order response".to_string(),
            ));
        }
        let mut records = first_page.records;
        for page in 2..=pages {
            if cancel.is_cancelled() {
                return Err(FetchStructureError::Cancelled);
            }
            let response = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(FetchStructureError::Cancelled),
                r = self.transport.structure_market_orders(
                    access_token, location_id, solar_system_id, page, None,
                ) => r,
            }
            .map_err(|error| match error {
                EsiError::AccessDenied => FetchStructureError::AccessDenied,
                other => FetchStructureError::Other(other.to_string()),
            })?;
            if response.not_modified {
                return Err(FetchStructureError::Other(
                    "EVE returned an invalid market order response".to_string(),
                ));
            }
            records.extend(response.records);
        }
        Ok(records)
    }

    /// Splits one structure's combined order book by `type_id` and commits
    /// a batch per claimed candidate -- the fan-out half of "one fetch, many
    /// candidates".
    async fn complete_structure_batches(
        &self,
        target: StructureRefreshTarget,
        claimed: &[(MarketCoverageItem, DateTime<Utc>)],
        records: Vec<MarketOrderObservation>,
    ) -> Vec<MarketRefreshOutcome> {
        let StructureRefreshTarget {
            workspace_id,
            source_id,
            scope,
            location_id,
            solar_system_id,
        } = target;
        let observed_at = Utc::now();
        let next_refresh_at = observed_at
            + Duration::from_std(self.refresh_interval).unwrap_or_else(|_| Duration::minutes(5));

        let mut by_type: BTreeMap<i64, Vec<MarketOrderObservation>> = BTreeMap::new();
        for record in records {
            // Same "skip just this one order" tolerance `fetch_type` applies
            // for the public path -- ESI occasionally returns an order with
            // duration=0.
            if record.duration_days == 0 {
                tracing::warn!(
                    type_id = record.type_id,
                    order_id = record.order_id,
                    "skipping esi market order with non-positive duration"
                );
                continue;
            }
            by_type.entry(record.type_id).or_default().push(record);
        }

        let mut outcomes = Vec::with_capacity(claimed.len());
        for (item, claim) in claimed {
            let raw_orders = by_type.remove(&item.type_id).unwrap_or_default();
            let mut orders = Vec::with_capacity(raw_orders.len());
            let mut parse_failed = false;
            for record in raw_orders {
                match to_core_order(record, Some(location_id)) {
                    Ok(order) => orders.push(order),
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(
                            %error,
                            source_id = %source_id.0,
                            item_name = %item.type_name,
                            type_id = item.type_id,
                            "market refresh failed"
                        );
                        if let Err(persistence_error) = self
                            .repository
                            .fail(
                                workspace_id,
                                source_id,
                                MarketRefreshFailure {
                                    type_id: item.type_id,
                                    claim: *claim,
                                    attempted_at: observed_at,
                                    next_refresh_at: observed_at + REFRESH_RETRY_DELAY,
                                    error_message: message,
                                },
                            )
                            .await
                        {
                            tracing::warn!(
                                %persistence_error,
                                type_id = item.type_id,
                                "market refresh failure recording failed"
                            );
                        }
                        outcomes.push(MarketRefreshOutcome::Failed);
                        parse_failed = true;
                        break;
                    }
                }
            }
            if parse_failed {
                continue;
            }
            let batch = EsiMarketObservationBatch {
                id: MarketObservationBatchId::new(),
                source_id,
                type_id: item.type_id,
                type_name: item.type_name.clone(),
                region_id: scope.region_id,
                solar_system_id,
                location_id,
                observed_at,
                etag: None,
                expires_at: None,
                orders,
            };
            match self
                .repository
                .complete(workspace_id, *claim, next_refresh_at, batch)
                .await
            {
                Ok(true) => outcomes.push(MarketRefreshOutcome::Succeeded),
                Ok(false) => outcomes.push(MarketRefreshOutcome::Skipped),
                Err(error) => {
                    let message = error.to_string();
                    tracing::warn!(
                        %error,
                        source_id = %source_id.0,
                        item_name = %item.type_name,
                        type_id = item.type_id,
                        "market refresh commit failed"
                    );
                    if let Err(persistence_error) = self
                        .repository
                        .fail(
                            workspace_id,
                            source_id,
                            MarketRefreshFailure {
                                type_id: item.type_id,
                                claim: *claim,
                                attempted_at: observed_at,
                                next_refresh_at: observed_at + REFRESH_RETRY_DELAY,
                                error_message: message,
                            },
                        )
                        .await
                    {
                        tracing::warn!(
                            %persistence_error,
                            type_id = item.type_id,
                            "market refresh failure recording failed"
                        );
                    }
                    outcomes.push(MarketRefreshOutcome::Failed);
                }
            }
        }
        outcomes
    }
}
