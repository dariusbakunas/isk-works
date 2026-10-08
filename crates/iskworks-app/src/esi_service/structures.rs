use super::*;

impl EsiApplicationService {
    pub async fn resolve_structures(
        &self,
        workspace_id: WorkspaceId,
        structure_ids: &[i64],
    ) -> Result<StructureResolution, EsiApplicationError> {
        if structure_ids.is_empty() || structure_ids.len() > 50 {
            return Err(InventoryError::Validation(
                "Select between 1 and 50 structure IDs to resolve.".to_string(),
            )
            .into());
        }
        let mut unresolved = structure_ids
            .iter()
            .copied()
            .filter(|id| *id > 0)
            .collect::<BTreeSet<_>>();
        if unresolved.len() != structure_ids.len() {
            return Err(InventoryError::Validation(
                "Structure IDs must be positive and unique.".to_string(),
            )
            .into());
        }
        let connections = self.repository.list_connections(workspace_id).await?;
        let eligible = connections
            .iter()
            .filter(|connection| {
                connection.status != ConnectionStatus::Disconnected
                    && connection
                        .granted_scopes
                        .iter()
                        .any(|scope| scope == STRUCTURE_SCOPE)
            })
            .cloned()
            .collect::<Vec<_>>();
        let needs_reconnection = eligible.is_empty();
        let mut resolved = Vec::new();
        let mut warnings = Vec::new();
        for candidate in &eligible {
            if unresolved.is_empty() {
                break;
            }
            let now = Utc::now();
            let to_look_up = self
                .repository
                .structures_to_look_up(
                    candidate,
                    &unresolved.iter().copied().collect::<Vec<_>>(),
                    None,
                    now,
                )
                .await?;
            if to_look_up.is_empty() {
                continue;
            }
            let (connection, access_token) = match self.refresh(candidate.id).await {
                Ok(value) => value,
                Err(error) => {
                    warnings.push(format!(
                        "{} could not be refreshed for structure lookup: {error}",
                        candidate.character_name
                    ));
                    continue;
                }
            };
            let mut denied = Vec::new();
            for structure_id in to_look_up {
                match self.transport.structure(&access_token, structure_id).await {
                    Ok(structure) => {
                        unresolved.remove(&structure_id);
                        resolved.push(ResolvedStructure {
                            structure,
                            connection_id: connection.id,
                            character_name: connection.character_name.clone(),
                        });
                    }
                    Err(EsiError::AccessDenied | EsiError::PermanentFailure) => {
                        denied.push(structure_id);
                    }
                    Err(error) => {
                        warnings.push(format!(
                            "{} could not resolve structure {structure_id}: {error}",
                            connection.character_name
                        ));
                    }
                }
            }
            self.repository
                .record_structure_lookup_denials(&connection, &denied, now + STRUCTURE_DENIAL_TTL)
                .await?;
        }
        Ok(StructureResolution {
            resolved,
            unresolved_structure_ids: unresolved.into_iter().collect(),
            eligible_character_count: eligible.len() as u64,
            needs_reconnection,
            warnings,
        })
    }

    /// Looks up names for whichever of `structure_ids` need one: no name,
    /// or one older than `STRUCTURE_NAME_REFRESH`, and not recently denied
    /// to this character. 403/404 answers are remembered for
    /// `STRUCTURE_DENIAL_TTL` so the next sync doesn't spend ESI's error
    /// budget asking again.
    pub(super) async fn resolve_and_cache_structures(
        &self,
        connection: &ConnectedCharacter,
        token: &str,
        structure_ids: impl IntoIterator<Item = i64>,
    ) -> Result<(), EsiApplicationError> {
        let now = Utc::now();
        let candidates = structure_ids.into_iter().collect::<BTreeSet<_>>();
        let to_look_up = self
            .repository
            .structures_to_look_up(
                connection,
                &candidates.into_iter().collect::<Vec<_>>(),
                Some(now - STRUCTURE_NAME_REFRESH),
                now,
            )
            .await?;
        let mut resolved = Vec::new();
        let mut denied = Vec::new();
        for structure_id in to_look_up {
            match self.transport.structure(token, structure_id).await {
                Ok(structure) => resolved.push(structure),
                Err(EsiError::AccessDenied | EsiError::PermanentFailure) => {
                    denied.push(structure_id);
                }
                // ESI asked us to slow down; the rest can wait for next sync.
                Err(
                    EsiError::RateLimited { .. }
                    | EsiError::EsiErrorLimit { .. }
                    | EsiError::ServerDowntime { .. },
                ) => break,
                Err(_) => {}
            }
        }
        self.repository
            .cache_structure_names(connection, &resolved)
            .await?;
        self.repository
            .record_structure_lookup_denials(connection, &denied, now + STRUCTURE_DENIAL_TTL)
            .await?;
        Ok(())
    }
}

pub(super) fn asset_structure_candidates(records: &[AssetObservation]) -> BTreeSet<i64> {
    let item_ids = records
        .iter()
        .map(|asset| asset.item_id)
        .collect::<BTreeSet<_>>();
    records
        .iter()
        .filter(|asset| asset.location_type == "item")
        .map(|asset| asset.location_id)
        .filter(|location_id| *location_id >= 1_000_000_000_000 && !item_ids.contains(location_id))
        .collect()
}

/// How long a structure a character got 403/404 for is not asked about
/// again by that character. Docking access rarely changes; a day-scale wait
/// keeps inaccessible structures from spending ESI's error budget each sync.
pub(super) const STRUCTURE_DENIAL_TTL: Duration = Duration::hours(12);
/// Known structure names are refreshed this often (structures get renamed).
pub(super) const STRUCTURE_NAME_REFRESH: Duration = Duration::days(7);
