use super::*;

impl EsiApplicationService {
    pub async fn sync(
        &self,
        id: ConnectedCharacterId,
        kind: EsiSyncKind,
    ) -> Result<Vec<EsiSyncRun>, EsiApplicationError> {
        self.manual_sync_gate
            .try_begin(id, kind)
            .map_err(|retry_after_seconds| EsiApplicationError::SyncTooSoon {
                retry_after_seconds,
            })?;
        let (connection, token) = self.refresh(id).await?;
        match kind {
            EsiSyncKind::Assets => Ok(vec![self.sync_assets(&connection, &token).await?]),
            EsiSyncKind::WalletTransactions => {
                Ok(vec![self.sync_wallet(&connection, &token).await?])
            }
            EsiSyncKind::AllSupported => {
                let assets = self.sync_assets(&connection, &token).await?;
                let wallet = self.sync_wallet(&connection, &token).await?;
                Ok(vec![assets, wallet])
            }
        }
    }

    pub(super) async fn sync_assets(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<EsiSyncRun, EsiApplicationError> {
        let run = self
            .repository
            .start_sync(connection, EsiSyncKind::Assets)
            .await?;
        if connection
            .granted_scopes
            .iter()
            .any(|scope| scope == STRUCTURE_SCOPE)
        {
            let structure_ids = self.repository.unresolved_structure_ids(connection).await?;
            self.resolve_and_cache_structures(connection, token, structure_ids)
                .await?;
        }
        let checkpoint_etag = self
            .repository
            .checkpoint_etag(connection.id, "assets")
            .await?;
        let first = match self
            .transport
            .assets(
                token,
                connection.eve_character_id,
                1,
                checkpoint_etag.as_deref(),
            )
            .await
        {
            Ok(value) => value,
            Err(error) => {
                self.repository
                    .fail_sync(run.id, error_code(&error), &error.to_string())
                    .await?;
                return Err(error.into());
            }
        };
        if first.not_modified {
            self.repository
                .record_sync_metadata(run.id, &first.metadata)
                .await?;
            return self
                .repository
                .complete_unchanged_sync(
                    &run,
                    "assets",
                    "Asset snapshot is unchanged from the latest complete synchronization.",
                )
                .await
                .map_err(Into::into);
        }
        let pages = first.metadata.pages.unwrap_or(1).max(1);
        self.repository
            .record_sync_metadata(run.id, &first.metadata)
            .await?;
        let etag = first.metadata.etag.clone();
        let mut records = first.records;
        for page in 2..=pages {
            match self
                .transport
                .assets(token, connection.eve_character_id, page, None)
                .await
            {
                Ok(response) => {
                    if response
                        .metadata
                        .pages
                        .is_some_and(|actual| actual != pages)
                    {
                        self.repository
                            .mark_incomplete_asset_sync(
                                &run,
                                pages,
                                &records,
                                "Asset page count changed during synchronization. The previous complete asset snapshot remains active.",
                            )
                            .await?;
                        return Err(EsiError::InvalidResponse.into());
                    }
                    records.extend(response.records);
                }
                Err(error) => {
                    self.repository
                        .mark_incomplete_asset_sync(
                            &run,
                            pages,
                            &records,
                            "Asset synchronization stopped before every page was imported. The previous complete asset snapshot remains active.",
                        )
                        .await?;
                    return Err(error.into());
                }
            }
        }
        let first_blueprints = self
            .transport
            .blueprints(token, connection.eve_character_id, 1)
            .await?;
        let blueprint_pages = first_blueprints.metadata.pages.unwrap_or(1).max(1);
        let mut blueprints = first_blueprints.records;
        for page in 2..=blueprint_pages {
            blueprints.extend(
                self.transport
                    .blueprints(token, connection.eve_character_id, page)
                    .await?
                    .records,
            );
        }
        if connection
            .granted_scopes
            .iter()
            .any(|scope| scope == STRUCTURE_SCOPE)
        {
            let mut structure_ids = asset_structure_candidates(&records);
            structure_ids.extend(
                blueprints
                    .iter()
                    .map(|blueprint| blueprint.location_id)
                    .filter(|location_id| *location_id >= 1_000_000_000_000),
            );
            self.resolve_and_cache_structures(connection, token, structure_ids)
                .await?;
        }
        // One unusable blueprint must not fail the whole sync: skip it, keep
        // its last good row, and say so on the run.
        let (blueprints, invalid): (Vec<_>, Vec<_>) = blueprints
            .into_iter()
            .partition(BlueprintAssetObservation::efficiency_in_range);
        for blueprint in &invalid {
            tracing::warn!(
                connection_id = %connection.id.0,
                item_id = blueprint.item_id,
                type_id = blueprint.type_id,
                material_efficiency = blueprint.material_efficiency,
                time_efficiency = blueprint.time_efficiency,
                "skipping blueprint with out-of-range ME/TE from ESI"
            );
        }
        let retained: Vec<i64> = invalid.iter().map(|blueprint| blueprint.item_id).collect();
        // Blueprints before assets: `complete_assets` commits the assets
        // ETag checkpoint, and a retry that gets 304 for assets returns
        // early -- so a blueprint failure after it would leave blueprints
        // stale until the assets themselves change.
        self.repository
            .complete_blueprints(connection, &blueprints, &retained)
            .await?;
        self.repository
            .complete_assets(&run, pages, &records, etag.as_deref(), invalid.len() as u64)
            .await?;
        self.repository
            .get_sync_run(run.id)
            .await
            .map_err(Into::into)
    }

    pub(super) async fn sync_wallet(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
    ) -> Result<EsiSyncRun, EsiApplicationError> {
        let run = self
            .repository
            .start_sync(connection, EsiSyncKind::WalletTransactions)
            .await?;
        match self
            .transport
            .wallet_balance(token, connection.eve_character_id)
            .await
        {
            Ok(response) => {
                self.repository
                    .record_sync_metadata(run.id, &response.metadata)
                    .await?;
                let balance = response
                    .records
                    .into_iter()
                    .next()
                    .ok_or(EsiError::InvalidResponse)?;
                let observed_at = Utc::now();
                self.repository
                    .save_wallet_balance(
                        &run,
                        balance.balance,
                        observed_at,
                        &format!("wallet-balance:{}", observed_at.timestamp_millis()),
                    )
                    .await?;
            }
            Err(error) => {
                tracing::warn!(
                    connection_id = %connection.id.0,
                    character_name = %connection.character_name,
                    error = %error,
                    "wallet balance refresh failed; transaction synchronization will continue"
                );
            }
        }
        let mut records = Vec::new();
        let mut from_id = None;
        let mut etag = self
            .repository
            .checkpoint_etag(connection.id, "wallet_transactions")
            .await?;
        for _ in 0..MAX_WALLET_REQUESTS {
            let response = match self
                .transport
                .wallet_transactions(token, connection.eve_character_id, from_id, etag.as_deref())
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    self.repository
                        .fail_sync(run.id, error_code(&error), &error.to_string())
                        .await?;
                    return Err(error.into());
                }
            };
            if response.not_modified {
                self.repository
                    .record_sync_metadata(run.id, &response.metadata)
                    .await?;
                let completed = self
                    .repository
                    .complete_unchanged_sync(
                        &run,
                        "wallet_transactions",
                        "Wallet transactions are unchanged from the latest synchronization.",
                    )
                    .await?;
                self.ingest_wallet_journal(connection, token).await;
                self.enrich_wallet_client_names(connection).await;
                return Ok(completed);
            }
            self.repository
                .record_sync_metadata(run.id, &response.metadata)
                .await?;
            etag = response.metadata.etag;
            let count = response.records.len();
            let next = response
                .records
                .iter()
                .map(|item| item.transaction_id)
                .min();
            records.extend(response.records);
            if count < WALLET_PAGE_LIMIT || next.is_none() {
                break;
            }
            from_id = next;
            etag = None;
        }
        self.repository
            .complete_wallet(&run, &records, etag.as_deref())
            .await?;
        self.ingest_wallet_journal(connection, token).await;
        self.enrich_wallet_client_names(connection).await;
        self.repository
            .get_sync_run(run.id)
            .await
            .map_err(Into::into)
    }

    /// Accumulate the wallet journal (fees, taxes, non-market flows). ESI only
    /// keeps about 30 days, so this runs on every wallet sync whether or not the
    /// transactions changed. Best-effort like the balance fetch: a journal
    /// failure is logged and never fails the transaction sync.
    async fn ingest_wallet_journal(&self, connection: &ConnectedCharacter, token: &str) {
        let mut entries = Vec::new();
        let mut page = 1;
        loop {
            let response = match self
                .transport
                .wallet_journal(token, connection.eve_character_id, page)
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(
                        connection_id = %connection.id.0,
                        character_name = %connection.character_name,
                        error = %error,
                        page,
                        "wallet journal fetch failed; keeping the pages already read"
                    );
                    break;
                }
            };
            let pages = response.metadata.pages.unwrap_or(1);
            entries.extend(response.records);
            if page >= pages || page >= MAX_JOURNAL_PAGES {
                break;
            }
            page += 1;
        }
        if entries.is_empty() {
            return;
        }
        match self
            .repository
            .save_wallet_journal(connection.id, &entries)
            .await
        {
            Ok(new_entries) => tracing::debug!(
                connection_id = %connection.id.0,
                fetched = entries.len(),
                new_entries,
                "wallet journal ingested"
            ),
            Err(error) => tracing::warn!(
                connection_id = %connection.id.0,
                error = %error,
                "wallet journal could not be stored"
            ),
        }
    }

    async fn enrich_wallet_client_names(&self, connection: &ConnectedCharacter) {
        let ids = match self
            .repository
            .unresolved_wallet_client_ids(connection.workspace_id)
            .await
        {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(
                    workspace_id = %connection.workspace_id.0,
                    error = %error,
                    "wallet client name enrichment lookup failed"
                );
                return;
            }
        };
        let lookup = lookup_entity_names(self.transport.as_ref(), &ids).await;
        if let Some(error) = &lookup.stopped_by {
            tracing::warn!(
                workspace_id = %connection.workspace_id.0,
                requested_count = ids.len(),
                error = %error,
                "wallet client name enrichment stopped early"
            );
        }
        if let Err(error) = self.repository.cache_entity_names(&lookup.names).await {
            tracing::warn!(
                workspace_id = %connection.workspace_id.0,
                resolved_count = lookup.names.len(),
                error = %error,
                "wallet client name cache update failed"
            );
        }
        if let Err(error) = self
            .repository
            .record_entity_name_misses(&lookup.misses)
            .await
        {
            tracing::warn!(
                workspace_id = %connection.workspace_id.0,
                miss_count = lookup.misses.len(),
                error = %error,
                "wallet client name miss update failed"
            );
        }
    }
}

/// `/universe/names` takes at most this many IDs per request.
pub(super) const NAME_LOOKUP_BATCH: usize = 1_000;
/// How many rejected `/universe/names` requests one lookup may spend
/// narrowing down which IDs ESI can't name. ESI rejects a whole batch with
/// 404 when any one ID is invalid; halving the batch isolates one bad ID out
/// of 1,000 in about ten requests.
pub(super) const NAME_LOOKUP_ERROR_BUDGET: usize = 12;

#[derive(Debug, Default)]
pub(super) struct EntityNameLookup {
    pub(super) names: Vec<iskworks_esi::EveEntityName>,
    /// IDs ESI couldn't name -- omitted from an answer, rejected on their
    /// own, or left in a rejected batch once the error budget ran out.
    pub(super) misses: Vec<i64>,
    /// Why the lookup stopped before asking about every ID (rate limits,
    /// outages); those IDs are neither named nor misses and are retried
    /// on a later sync.
    pub(super) stopped_by: Option<EsiError>,
}

/// Names `ids` with as few failing `/universe/names` requests as possible:
/// a rejected batch is split in half and retried until
/// `NAME_LOOKUP_ERROR_BUDGET` is spent.
pub(super) async fn lookup_entity_names(
    transport: &dyn EsiTransport,
    ids: &[i64],
) -> EntityNameLookup {
    let mut lookup = EntityNameLookup::default();
    let mut pending: std::collections::VecDeque<Vec<i64>> =
        ids.chunks(NAME_LOOKUP_BATCH).map(<[i64]>::to_vec).collect();
    let mut error_budget = NAME_LOOKUP_ERROR_BUDGET;
    while let Some(batch) = pending.pop_front() {
        match transport.universe_names(&batch).await {
            Ok(names) => {
                let named = names.iter().map(|name| name.id).collect::<BTreeSet<_>>();
                lookup
                    .misses
                    .extend(batch.iter().filter(|id| !named.contains(id)));
                lookup.names.extend(names);
            }
            Err(EsiError::PermanentFailure) if batch.len() > 1 && error_budget > 0 => {
                error_budget -= 1;
                let (left, right) = batch.split_at(batch.len() / 2);
                pending.push_front(right.to_vec());
                pending.push_front(left.to_vec());
            }
            Err(EsiError::PermanentFailure) => lookup.misses.extend(batch),
            Err(error) => {
                lookup.stopped_by = Some(error);
                break;
            }
        }
    }
    lookup
}
