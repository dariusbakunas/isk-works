use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    ValidatingSource,
    ReadingTypes,
    ReadingBlueprints,
    NormalizingManufacturingRecipes,
    WritingTypes,
    WritingBlueprintRecipes,
    ActivatingSdeVersion,
    Complete,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ProgressEvent {
    pub phase: ImportPhase,
    pub message: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
}

pub trait ProgressReporter: Send + Sync {
    fn report(&self, event: ProgressEvent);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopProgressReporter;

impl ProgressReporter for NoopProgressReporter {
    fn report(&self, _event: ProgressEvent) {}
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCounts {
    pub types: u64,
    pub categories: u64,
    pub groups: u64,
    pub meta_groups: u64,
    pub market_groups: u64,
    pub classified_types: u64,
    pub blueprints: u64,
    pub material_lines: u64,
    pub product_lines: u64,
    pub skipped_blueprints: u64,
    pub reaction_formulas: u64,
    pub reaction_material_lines: u64,
    pub reaction_product_lines: u64,
    pub skipped_reaction_formulas: u64,
}

#[derive(Debug, Clone)]
pub struct NewImport {
    pub source_version: String,
    pub source_label: String,
    pub source_checksum: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ImportOutcome {
    Imported {
        import_id: Uuid,
        source_version: String,
        counts: ImportCounts,
    },
    AlreadyActive {
        import_id: Uuid,
        source_version: String,
        counts: ImportCounts,
    },
}

/// The optional datasets an already-active import must contain before an
/// import of the same archive can be skipped. Each is set when the source
/// being imported carries that dataset, so an older import made without it
/// is redone rather than reused.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct SdeDatasetRequirements {
    pub solar_systems: bool,
    pub facility_dogma: bool,
    pub npc_stations: bool,
    pub reaction_formulas: bool,
    pub packaged_volumes: bool,
    pub classification_metadata: bool,
    pub planetary_data: bool,
}

#[async_trait]
pub trait SdeImportStore: Send + Sync {
    async fn active_by_checksum(
        &self,
        checksum: &str,
        requires: SdeDatasetRequirements,
    ) -> Result<Option<(Uuid, String, ImportCounts)>, SdeError>;
    async fn begin_import(&self, import: NewImport) -> Result<Uuid, SdeError>;
    async fn write_dataset(
        &self,
        import_id: Uuid,
        dataset: &NormalizedSde,
        progress: &dyn ProgressReporter,
    ) -> Result<(), SdeError>;
    async fn activate_import(
        &self,
        import_id: Uuid,
        counts: ImportCounts,
        completed_at: DateTime<Utc>,
    ) -> Result<(), SdeError>;
    async fn fail_import(&self, import_id: Uuid, error: &str) -> Result<(), SdeError>;
}

pub struct SdeImporter<'a> {
    store: &'a dyn SdeImportStore,
    progress: &'a dyn ProgressReporter,
}

impl<'a> SdeImporter<'a> {
    #[must_use]
    pub const fn new(store: &'a dyn SdeImportStore, progress: &'a dyn ProgressReporter) -> Self {
        Self { store, progress }
    }

    pub async fn import_zip(
        &self,
        path: impl AsRef<Path>,
        force: bool,
    ) -> Result<ImportOutcome, SdeError> {
        self.emit(ImportPhase::ValidatingSource, "Validating source");
        let source = SdeSource::open_zip(path)?;
        if !force {
            if let Some((import_id, source_version, counts)) = self
                .store
                .active_by_checksum(
                    &source.source_checksum,
                    SdeDatasetRequirements {
                        solar_systems: source.has_solar_systems(),
                        facility_dogma: source.has_facility_dogma(),
                        npc_stations: source.has_npc_stations(),
                        reaction_formulas: source.has_reaction_formulas(),
                        packaged_volumes: source.has_packaged_volumes(),
                        classification_metadata: source.has_classification_metadata(),
                        planetary_data: source.has_planetary_data(),
                    },
                )
                .await?
            {
                return Ok(ImportOutcome::AlreadyActive {
                    import_id,
                    source_version,
                    counts,
                });
            }
        }

        self.emit(ImportPhase::ReadingTypes, "Reading types");
        self.emit(ImportPhase::ReadingBlueprints, "Reading blueprints");
        self.emit(
            ImportPhase::NormalizingManufacturingRecipes,
            "Normalizing manufacturing recipes",
        );
        let dataset = parse(&source)?;
        let counts = dataset.counts();
        let import_id = self
            .store
            .begin_import(NewImport {
                source_version: dataset.source_version.clone(),
                source_label: dataset.source_label.clone(),
                source_checksum: dataset.source_checksum.clone(),
                started_at: Utc::now(),
            })
            .await?;

        if let Err(error) = self
            .store
            .write_dataset(import_id, &dataset, self.progress)
            .await
        {
            let _ = self.store.fail_import(import_id, &error.to_string()).await;
            return Err(error);
        }

        self.emit(ImportPhase::ActivatingSdeVersion, "Activating SDE version");
        if let Err(error) = self
            .store
            .activate_import(import_id, counts, Utc::now())
            .await
        {
            let _ = self.store.fail_import(import_id, &error.to_string()).await;
            return Err(error);
        }
        self.progress.report(ProgressEvent {
            phase: ImportPhase::Complete,
            message: "Import complete".to_string(),
            completed: Some(counts.blueprints),
            total: Some(counts.blueprints),
        });

        Ok(ImportOutcome::Imported {
            import_id,
            source_version: dataset.source_version,
            counts,
        })
    }

    fn emit(&self, phase: ImportPhase, message: &str) {
        self.progress.report(ProgressEvent {
            phase,
            message: message.to_string(),
            completed: None,
            total: None,
        });
    }
}
