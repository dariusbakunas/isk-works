//! `MarketService`: import preview/commit and price-preview orchestration
//! over the `MarketRepository` port. Ordering and validation are unchanged.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::Utc;

use crate::{PriceSourceId, WorkspaceId};

use super::depth::calculate_market_depth;
use super::errors::MarketError;
use super::import_parse::{parse_eve_client_market_export, validate_market_upload_batch};
use super::repository::MarketRepository;
use super::types::{
    market_freshness, MarketImportPreview, MarketImportPreviewFile, MarketImportResult,
    MarketPricePreview, MarketPricePreviewCommand, MarketUpload, ResolvedMarketExport,
    MAX_MARKET_ROWS_PER_BATCH,
};

#[derive(Clone)]
pub struct MarketService {
    repository: Arc<dyn MarketRepository>,
}

impl MarketService {
    #[must_use]
    pub fn new(repository: Arc<dyn MarketRepository>) -> Self {
        Self { repository }
    }

    pub async fn preview_import(
        &self,
        workspace_id: WorkspaceId,
        uploads: Vec<MarketUpload>,
    ) -> Result<MarketImportPreview, MarketError> {
        validate_market_upload_batch(&uploads)?;
        let imported_at = Utc::now();
        let mut parsed_files = Vec::new();
        let mut previews = Vec::new();
        let mut total_rows = 0_usize;
        for upload in uploads {
            match parse_eve_client_market_export(&upload, imported_at) {
                Ok(exports) => {
                    for parsed in exports {
                        total_rows = total_rows
                            .checked_add(parsed.orders.len())
                            .ok_or(MarketError::TooLarge)?;
                        if total_rows > MAX_MARKET_ROWS_PER_BATCH {
                            return Err(MarketError::TooLarge);
                        }
                        let type_name = self
                            .repository
                            .resolve_type_name(parsed.type_id)
                            .await?
                            .ok_or(MarketError::UnknownType);
                        match type_name {
                            Ok(type_name) => {
                                parsed_files.push((parsed.file_checksum.clone(), previews.len()));
                                previews
                                    .push(MarketImportPreviewFile::from_parsed(&parsed, type_name));
                            }
                            Err(error) => previews.push(MarketImportPreviewFile::failed(
                                parsed.safe_filename,
                                parsed.file_size_bytes,
                                error.problem(),
                            )),
                        }
                    }
                }
                Err(error) => previews.push(MarketImportPreviewFile::failed(
                    upload
                        .filename
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or("market-export")
                        .to_string(),
                    upload.content.len() as u64,
                    error.problem(),
                )),
            }
        }
        let checksums: Vec<_> = parsed_files
            .iter()
            .map(|(checksum, _)| checksum.clone())
            .collect();
        let imported = self
            .repository
            .imported_file_checksums(workspace_id, &checksums)
            .await?;
        for (checksum, index) in parsed_files {
            if imported.contains(&checksum) {
                previews[index].already_imported = true;
                previews[index].can_import = false;
                previews[index]
                    .warnings
                    .push("This exact file was already imported.".to_string());
            }
        }
        let location_ids = previews
            .iter()
            .filter_map(|file| file.location_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let location_names = self
            .repository
            .location_names(workspace_id, &location_ids)
            .await?;
        for file in &mut previews {
            if let Some(name) = file
                .location_id
                .and_then(|location_id| location_names.get(&location_id))
            {
                file.location_name = Some(name.clone());
            }
        }
        Ok(MarketImportPreview::from_files(previews))
    }

    pub async fn import(
        &self,
        workspace_id: WorkspaceId,
        uploads: Vec<MarketUpload>,
    ) -> Result<MarketImportResult, MarketError> {
        validate_market_upload_batch(&uploads)?;
        let imported_at = Utc::now();
        let mut valid = Vec::new();
        let mut failed = Vec::new();
        let mut total_rows = 0_usize;
        for upload in uploads {
            match parse_eve_client_market_export(&upload, imported_at) {
                Ok(exports) => {
                    for parsed in exports {
                        total_rows = total_rows
                            .checked_add(parsed.orders.len())
                            .ok_or(MarketError::TooLarge)?;
                        if total_rows > MAX_MARKET_ROWS_PER_BATCH {
                            return Err(MarketError::TooLarge);
                        }
                        match self.repository.resolve_type_name(parsed.type_id).await? {
                            Some(type_name) => {
                                valid.push(ResolvedMarketExport { parsed, type_name })
                            }
                            None => failed.push(MarketImportPreviewFile::failed(
                                parsed.safe_filename,
                                parsed.file_size_bytes,
                                MarketError::UnknownType.problem(),
                            )),
                        }
                    }
                }
                Err(error) => failed.push(MarketImportPreviewFile::failed(
                    upload
                        .filename
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or("market-export")
                        .to_string(),
                    upload.content.len() as u64,
                    error.problem(),
                )),
            }
        }
        let checksums: Vec<_> = valid
            .iter()
            .map(|file| file.parsed.file_checksum.clone())
            .collect();
        let imported = self
            .repository
            .imported_file_checksums(workspace_id, &checksums)
            .await?;
        let skipped_duplicate_files = imported.len() as u64;
        valid.retain(|file| !imported.contains(&file.parsed.file_checksum));
        if valid.is_empty() {
            return Ok(MarketImportResult {
                batch: None,
                imported_files: 0,
                skipped_duplicate_files,
                failed_files: failed,
                imported_observations: 0,
                warnings: vec!["No new market files were imported.".to_string()],
            });
        }
        let warnings = if failed.is_empty() {
            Vec::new()
        } else {
            vec!["Valid files were imported; invalid files were left unchanged.".to_string()]
        };
        let batch = self
            .repository
            .commit_import(
                workspace_id,
                valid,
                skipped_duplicate_files,
                warnings.clone(),
            )
            .await?;
        Ok(MarketImportResult {
            imported_files: batch.file_count,
            skipped_duplicate_files,
            failed_files: failed,
            imported_observations: batch.observation_count,
            batch: Some(batch),
            warnings,
        })
    }

    pub async fn preview_price(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        command: MarketPricePreviewCommand,
    ) -> Result<MarketPricePreview, MarketError> {
        let source = self.preview_source(workspace_id, source_id).await?;
        let book = self
            .repository
            .get_source_order_book(
                workspace_id,
                source_id,
                command.type_id,
                source.config.location_id,
                source.config.pinned_batch_id,
            )
            .await?;
        Self::preview_from_book(&source, source_id, book, &command)
    }

    /// `preview_price` for many types against one source: the source and
    /// every order book are read once (`get_source_order_books`) instead of
    /// once per type. Results line up with `commands`; `None` is a type
    /// with no order book -- `preview_price`'s `OrdersUnavailable`. The
    /// first error in `commands` order is returned, as calling
    /// `preview_price` per command would.
    pub async fn preview_prices(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        commands: &[MarketPricePreviewCommand],
    ) -> Result<Vec<Option<MarketPricePreview>>, MarketError> {
        if commands.is_empty() {
            return Ok(Vec::new());
        }
        let source = self.preview_source(workspace_id, source_id).await?;
        let type_ids: Vec<i64> = commands.iter().map(|command| command.type_id).collect();
        let books = self
            .repository
            .get_source_order_books(
                workspace_id,
                source_id,
                &type_ids,
                source.config.location_id,
                source.config.pinned_batch_id,
            )
            .await?;
        commands
            .iter()
            .map(|command| {
                books
                    .get(&command.type_id)
                    .cloned()
                    .map(|book| Self::preview_from_book(&source, source_id, book, command))
                    .transpose()
            })
            .collect()
    }

    async fn preview_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<super::types::MarketPriceSource, MarketError> {
        let source = self
            .repository
            .get_market_price_source(workspace_id, source_id)
            .await?;
        if source.config.archived_at.is_some() {
            return Err(MarketError::PriceSourceArchived);
        }
        Ok(source)
    }

    fn preview_from_book(
        source: &super::types::MarketPriceSource,
        source_id: PriceSourceId,
        book: super::types::MarketOrderBook,
        command: &MarketPricePreviewCommand,
    ) -> Result<MarketPricePreview, MarketError> {
        let depth = calculate_market_depth(
            &book.orders,
            source.config.pricing_policy,
            command.requested_quantity,
        )?;
        // Effective freshness for the state; `observed_at` stays the
        // physical fetch time (provenance) on the preview DTO.
        let freshness = market_freshness(
            book.effective_observed_at(),
            Utc::now(),
            source.config.fresh_after_hours,
            source.config.stale_after_hours,
        );
        Ok(MarketPricePreview {
            source_id,
            type_id: book.type_id,
            type_name: book.type_name,
            location_id: book.location_id,
            location_name: book.location_name,
            observed_at: book.observed_at,
            freshness,
            depth,
        })
    }
}
