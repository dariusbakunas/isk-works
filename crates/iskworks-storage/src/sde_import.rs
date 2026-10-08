use super::sde_read::{map_sde_sqlx_error, ImportIdentityRow};
use super::*;
use iskworks_sde::SdeDatasetRequirements;

#[derive(Clone)]
pub struct PgSdeRepository {
    pub(super) pool: PgPool,
}

impl PgSdeRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        Ok(Self::new(pool))
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl SdeImportStore for PgSdeRepository {
    async fn active_by_checksum(
        &self,
        checksum: &str,
        requires: SdeDatasetRequirements,
    ) -> Result<Option<(Uuid, String, ImportCounts)>, SdeError> {
        let SdeDatasetRequirements {
            solar_systems: requires_solar_systems,
            facility_dogma: requires_facility_dogma,
            npc_stations: requires_npc_stations,
            reaction_formulas: requires_reaction_formulas,
            packaged_volumes: requires_packaged_volumes,
            classification_metadata: requires_classification_metadata,
            planetary_data: requires_planetary_data,
        } = requires;
        let row = sqlx::query_as::<_, ImportIdentityRow>(
            r#"
            SELECT id, source_version, type_count, blueprint_count,
                   material_line_count, product_line_count, skipped_blueprint_count,
                   reaction_formula_count, reaction_material_line_count,
                   reaction_product_line_count, skipped_reaction_formula_count,
                   category_count, group_count, meta_group_count, market_group_count,
                   classified_type_count
            FROM sde_imports
            WHERE active = true
              AND source_checksum = $1
              AND (
                $2 = false
                OR EXISTS (
                  SELECT 1
                  FROM sde_solar_systems systems
                  WHERE systems.import_id = sde_imports.id
                    AND (
                      systems.security_status IS NOT NULL
                      OR systems.wormhole_class_id IS NOT NULL
                    )
                )
              )
              AND (
                $3 = false
                OR EXISTS (
                  SELECT 1
                  FROM sde_structure_manufacturing_modifiers modifiers
                  WHERE modifiers.import_id = sde_imports.id
                    AND modifiers.structure_size IS NOT NULL
                )
                AND EXISTS (
                  SELECT 1
                  FROM sde_structure_rig_modifiers rigs
                  WHERE rigs.import_id = sde_imports.id
                    AND rigs.rig_size IS NOT NULL
                )
              )
              AND (
                $4 = false
                OR EXISTS (
                  SELECT 1
                  FROM sde_npc_stations stations
                  WHERE stations.import_id = sde_imports.id
                )
              )
              AND (
                $5 = false
                OR EXISTS (
                  SELECT 1
                  FROM sde_reaction_formulas formulas
                  WHERE formulas.import_id = sde_imports.id
                )
              )
              AND (
                $6 = false
                OR EXISTS (
                  SELECT 1
                  FROM sde_types types
                  WHERE types.import_id = sde_imports.id
                    AND types.packaged_volume_m3 IS NOT NULL
                )
              )
              AND ($7 = false OR (
                category_count > 0 AND group_count > 0 AND meta_group_count > 0
                AND market_group_count > 0 AND classified_type_count > 0
              ))
              AND (
                $8 = false
                OR EXISTS (
                  SELECT 1
                  FROM sde_planet_schematics schematics
                  WHERE schematics.import_id = sde_imports.id
                )
              )
            "#,
        )
        .bind(checksum)
        .bind(requires_solar_systems)
        .bind(requires_facility_dogma)
        .bind(requires_npc_stations)
        .bind(requires_reaction_formulas)
        .bind(requires_packaged_volumes)
        .bind(requires_classification_metadata)
        .bind(requires_planetary_data)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        Ok(row.map(|row| {
            (
                row.id,
                row.source_version,
                ImportCounts {
                    types: row.type_count as u64,
                    blueprints: row.blueprint_count as u64,
                    material_lines: row.material_line_count as u64,
                    product_lines: row.product_line_count as u64,
                    skipped_blueprints: row.skipped_blueprint_count as u64,
                    reaction_formulas: row.reaction_formula_count as u64,
                    reaction_material_lines: row.reaction_material_line_count as u64,
                    reaction_product_lines: row.reaction_product_line_count as u64,
                    skipped_reaction_formulas: row.skipped_reaction_formula_count as u64,
                    categories: row.category_count as u64,
                    groups: row.group_count as u64,
                    meta_groups: row.meta_group_count as u64,
                    market_groups: row.market_group_count as u64,
                    classified_types: row.classified_type_count as u64,
                },
            )
        }))
    }

    async fn begin_import(&self, import: NewImport) -> Result<Uuid, SdeError> {
        let import_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO sde_imports (
              id, source_version, source_label, source_checksum, status, active, started_at
            )
            VALUES ($1, $2, $3, $4, 'importing', false, $5)
            "#,
        )
        .bind(import_id)
        .bind(import.source_version)
        .bind(import.source_label)
        .bind(import.source_checksum)
        .bind(import.started_at)
        .execute(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(import_id)
    }

    async fn write_dataset(
        &self,
        import_id: Uuid,
        dataset: &NormalizedSde,
        progress: &dyn ProgressReporter,
    ) -> Result<(), SdeError> {
        const TYPE_BATCH_SIZE: usize = 1_000;
        const RECIPE_BATCH_SIZE: usize = 1_000;

        for batch in dataset.categories.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_categories (import_id, category_id, name_en, published) ",
            );
            query.push_values(batch, |mut row, item| {
                row.push_bind(import_id)
                    .push_bind(item.category_id)
                    .push_bind(&item.name)
                    .push_bind(item.published);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }
        for batch in dataset.groups.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_groups (import_id, group_id, name_en, category_id, published) ",
            );
            query.push_values(batch, |mut row, item| {
                row.push_bind(import_id)
                    .push_bind(item.group_id)
                    .push_bind(&item.name)
                    .push_bind(item.category_id)
                    .push_bind(item.published);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }
        for batch in dataset.meta_groups.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_meta_groups (import_id, meta_group_id, name_en) ",
            );
            query.push_values(batch, |mut row, item| {
                row.push_bind(import_id)
                    .push_bind(item.meta_group_id)
                    .push_bind(&item.name);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }
        for batch in dataset.market_groups.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new("INSERT INTO sde_market_groups (import_id, market_group_id, name_en, parent_group_id) ");
            query.push_values(batch, |mut row, item| {
                row.push_bind(import_id)
                    .push_bind(item.market_group_id)
                    .push_bind(&item.name)
                    .push_bind(item.parent_group_id);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for (batch_index, batch) in dataset.types.chunks(TYPE_BATCH_SIZE).enumerate() {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_types \
                 (import_id, type_id, name_en, group_id, group_name_en, market_group_id, meta_group_id, packaged_volume_m3, published) ",
            );
            query.push_values(batch, |mut row, item| {
                row.push_bind(import_id)
                    .push_bind(item.type_id)
                    .push_bind(&item.name)
                    .push_bind(item.group_id)
                    .push_bind(&item.group_name)
                    .push_bind(item.market_group_id)
                    .push_bind(item.meta_group_id)
                    .push_bind(item.packaged_volume_m3)
                    .push_bind(item.published);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
            progress.report(ProgressEvent {
                phase: ImportPhase::WritingTypes,
                message: "Writing types".to_string(),
                completed: Some(
                    ((batch_index + 1) * TYPE_BATCH_SIZE).min(dataset.types.len()) as u64,
                ),
                total: Some(dataset.types.len() as u64),
            });
        }

        for batch in dataset.solar_systems.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_solar_systems \
                 (import_id, solar_system_id, name_en, constellation_id, region_id, security_status, wormhole_class_id) ",
            );
            query.push_values(batch, |mut row, system| {
                row.push_bind(import_id)
                    .push_bind(system.solar_system_id)
                    .push_bind(&system.name)
                    .push_bind(system.constellation_id)
                    .push_bind(system.region_id)
                    .push_bind(system.security_status)
                    .push_bind(system.wormhole_class_id);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.regions.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_regions (import_id, region_id, name_en) ",
            );
            query.push_values(batch, |mut row, region| {
                row.push_bind(import_id)
                    .push_bind(region.region_id)
                    .push_bind(&region.name);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.constellations.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_constellations \
                 (import_id, constellation_id, name_en, region_id) ",
            );
            query.push_values(batch, |mut row, constellation| {
                row.push_bind(import_id)
                    .push_bind(constellation.constellation_id)
                    .push_bind(&constellation.name)
                    .push_bind(constellation.region_id);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.npc_stations.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_npc_stations \
                 (import_id, station_id, name_en, solar_system_id, owner_corporation_id, station_type_id) ",
            );
            query.push_values(batch, |mut row, station| {
                row.push_bind(import_id)
                    .push_bind(station.station_id)
                    .push_bind(&station.name)
                    .push_bind(station.solar_system_id)
                    .push_bind(station.owner_corporation_id)
                    .push_bind(station.station_type_id);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.structure_modifiers.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_structure_manufacturing_modifiers \
                 (import_id, type_id, material_reduction_percent, time_reduction_percent, job_cost_reduction_percent, structure_size) ",
            );
            query.push_values(batch, |mut row, modifiers| {
                row.push_bind(import_id)
                    .push_bind(modifiers.type_id)
                    .push_bind(modifiers.material_reduction_percent)
                    .push_bind(modifiers.time_reduction_percent)
                    .push_bind(modifiers.job_cost_reduction_percent)
                    .push_bind(modifiers.structure_size);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.rig_modifiers.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_structure_rig_modifiers \
                 (import_id, type_id, material_reduction_percent, time_reduction_percent, \
                  high_sec_multiplier, low_sec_multiplier, null_sec_multiplier, compatible_structure_group_ids, rig_size, \
                  material_filter_ids, time_filter_ids) ",
            );
            query.push_values(batch, |mut row, modifiers| {
                row.push_bind(import_id)
                    .push_bind(modifiers.type_id)
                    .push_bind(modifiers.material_reduction_percent)
                    .push_bind(modifiers.time_reduction_percent)
                    .push_bind(modifiers.high_sec_multiplier)
                    .push_bind(modifiers.low_sec_multiplier)
                    .push_bind(modifiers.null_sec_multiplier)
                    .push_bind(&modifiers.compatible_structure_group_ids)
                    .push_bind(modifiers.rig_size)
                    .push_bind(&modifiers.material_filter_ids)
                    .push_bind(&modifiers.time_filter_ids);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.reaction_rig_modifiers.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_reaction_rig_modifiers \
                 (import_id, type_id, material_reduction_percent, time_reduction_percent, \
                  high_sec_multiplier, low_sec_multiplier, null_sec_multiplier, compatible_structure_group_ids, rig_size, \
                  material_filter_ids, time_filter_ids) ",
            );
            query.push_values(batch, |mut row, modifiers| {
                row.push_bind(import_id)
                    .push_bind(modifiers.type_id)
                    .push_bind(modifiers.material_reduction_percent)
                    .push_bind(modifiers.time_reduction_percent)
                    .push_bind(modifiers.high_sec_multiplier)
                    .push_bind(modifiers.low_sec_multiplier)
                    .push_bind(modifiers.null_sec_multiplier)
                    .push_bind(&modifiers.compatible_structure_group_ids)
                    .push_bind(modifiers.rig_size)
                    .push_bind(&modifiers.material_filter_ids)
                    .push_bind(&modifiers.time_filter_ids);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.industry_target_filters.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_industry_target_filters \
                 (import_id, filter_id, name_en, category_ids, group_ids) ",
            );
            query.push_values(batch, |mut row, filter| {
                row.push_bind(import_id)
                    .push_bind(filter.filter_id)
                    .push_bind(&filter.name)
                    .push_bind(&filter.category_ids)
                    .push_bind(&filter.group_ids);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        for batch in dataset.blueprints.chunks(RECIPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprints \
                 (import_id, blueprint_type_id, name_en, duration_seconds) ",
            );
            query.push_values(batch, |mut row, blueprint| {
                row.push_bind(import_id)
                    .push_bind(blueprint.blueprint_type_id)
                    .push_bind(&blueprint.name)
                    .push_bind(blueprint.duration_seconds);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        let materials = dataset
            .blueprints
            .iter()
            .flat_map(|blueprint| {
                blueprint
                    .materials
                    .iter()
                    .map(move |line| (blueprint.blueprint_type_id, line))
            })
            .collect::<Vec<_>>();
        for batch in materials.chunks(RECIPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_materials \
                 (import_id, blueprint_type_id, material_type_id, quantity, position) ",
            );
            query.push_values(batch, |mut row, (blueprint_type_id, line)| {
                row.push_bind(import_id)
                    .push_bind(*blueprint_type_id)
                    .push_bind(line.type_id)
                    .push_bind(line.quantity)
                    .push_bind(line.position);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        let products = dataset
            .blueprints
            .iter()
            .flat_map(|blueprint| {
                blueprint
                    .products
                    .iter()
                    .map(move |line| (blueprint.blueprint_type_id, line))
            })
            .collect::<Vec<_>>();
        for (batch_index, batch) in products.chunks(RECIPE_BATCH_SIZE).enumerate() {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_blueprint_products \
                 (import_id, blueprint_type_id, product_type_id, quantity, position) ",
            );
            query.push_values(batch, |mut row, (blueprint_type_id, line)| {
                row.push_bind(import_id)
                    .push_bind(*blueprint_type_id)
                    .push_bind(line.type_id)
                    .push_bind(line.quantity)
                    .push_bind(line.position);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
            progress.report(ProgressEvent {
                phase: ImportPhase::WritingBlueprintRecipes,
                message: "Writing blueprint recipes".to_string(),
                completed: Some(((batch_index + 1) * RECIPE_BATCH_SIZE).min(products.len()) as u64),
                total: Some(products.len() as u64),
            });
        }

        for batch in dataset.reaction_formulas.chunks(RECIPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_reaction_formulas \
                 (import_id, reaction_formula_type_id, name_en, duration_seconds) ",
            );
            query.push_values(batch, |mut row, formula| {
                row.push_bind(import_id)
                    .push_bind(formula.reaction_formula_type_id)
                    .push_bind(&formula.name)
                    .push_bind(formula.duration_seconds);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        let reaction_materials = dataset
            .reaction_formulas
            .iter()
            .flat_map(|formula| {
                formula
                    .materials
                    .iter()
                    .map(move |line| (formula.reaction_formula_type_id, line))
            })
            .collect::<Vec<_>>();
        for batch in reaction_materials.chunks(RECIPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_reaction_formula_materials \
                 (import_id, reaction_formula_type_id, material_type_id, quantity, position) ",
            );
            query.push_values(batch, |mut row, (reaction_formula_type_id, line)| {
                row.push_bind(import_id)
                    .push_bind(*reaction_formula_type_id)
                    .push_bind(line.type_id)
                    .push_bind(line.quantity)
                    .push_bind(line.position);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        let reaction_products = dataset
            .reaction_formulas
            .iter()
            .flat_map(|formula| {
                formula
                    .products
                    .iter()
                    .map(move |line| (formula.reaction_formula_type_id, line))
            })
            .collect::<Vec<_>>();
        for (batch_index, batch) in reaction_products.chunks(RECIPE_BATCH_SIZE).enumerate() {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_reaction_formula_products \
                 (import_id, reaction_formula_type_id, product_type_id, quantity, position) ",
            );
            query.push_values(batch, |mut row, (reaction_formula_type_id, line)| {
                row.push_bind(import_id)
                    .push_bind(*reaction_formula_type_id)
                    .push_bind(line.type_id)
                    .push_bind(line.quantity)
                    .push_bind(line.position);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
            progress.report(ProgressEvent {
                phase: ImportPhase::WritingBlueprintRecipes,
                message: "Writing reaction formula recipes".to_string(),
                completed: Some(
                    ((batch_index + 1) * RECIPE_BATCH_SIZE).min(reaction_products.len()) as u64,
                ),
                total: Some(reaction_products.len() as u64),
            });
        }

        for batch in dataset.planet_schematics.chunks(RECIPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_planet_schematics \
                 (import_id, schematic_id, name_en, cycle_time_seconds) ",
            );
            query.push_values(batch, |mut row, schematic| {
                row.push_bind(import_id)
                    .push_bind(schematic.schematic_id)
                    .push_bind(&schematic.name)
                    .push_bind(schematic.cycle_time_seconds);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }
        let schematic_types = dataset
            .planet_schematics
            .iter()
            .flat_map(|schematic| {
                schematic
                    .types
                    .iter()
                    .map(move |line| (schematic.schematic_id, line))
            })
            .collect::<Vec<_>>();
        for batch in schematic_types.chunks(RECIPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_planet_schematic_types \
                 (import_id, schematic_id, type_id, is_input, quantity) ",
            );
            query.push_values(batch, |mut row, (schematic_id, line)| {
                row.push_bind(import_id)
                    .push_bind(*schematic_id)
                    .push_bind(line.type_id)
                    .push_bind(line.is_input)
                    .push_bind(line.quantity);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }
        for batch in dataset.planets.chunks(TYPE_BATCH_SIZE) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sde_planets \
                 (import_id, planet_id, solar_system_id, celestial_index, name_en) ",
            );
            query.push_values(batch, |mut row, planet| {
                row.push_bind(import_id)
                    .push_bind(planet.planet_id)
                    .push_bind(planet.solar_system_id)
                    .push_bind(planet.celestial_index)
                    .push_bind(&planet.name);
            });
            query
                .build()
                .execute(&self.pool)
                .await
                .map_err(map_sde_sqlx_error)?;
        }

        self.rebuild_type_categories(import_id).await?;
        Ok(())
    }

    async fn activate_import(
        &self,
        import_id: Uuid,
        counts: ImportCounts,
        completed_at: DateTime<Utc>,
    ) -> Result<(), SdeError> {
        let mut tx = self.pool.begin().await.map_err(map_sde_sqlx_error)?;
        sqlx::query(
            "UPDATE sde_imports SET active = false, status = 'superseded' WHERE active = true",
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sde_sqlx_error)?;
        let result = sqlx::query(
            r#"
            UPDATE sde_imports
            SET active = true,
                status = 'active',
                completed_at = $2,
                type_count = $3,
                blueprint_count = $4,
                material_line_count = $5,
                product_line_count = $6,
                skipped_blueprint_count = $7,
                reaction_formula_count = $8,
                reaction_material_line_count = $9,
                reaction_product_line_count = $10,
                skipped_reaction_formula_count = $11,
                category_count = $12,
                group_count = $13,
                meta_group_count = $14,
                market_group_count = $15,
                classified_type_count = $16,
                error_message = NULL
            WHERE id = $1 AND status = 'importing'
            "#,
        )
        .bind(import_id)
        .bind(completed_at)
        .bind(counts.types as i64)
        .bind(counts.blueprints as i64)
        .bind(counts.material_lines as i64)
        .bind(counts.product_lines as i64)
        .bind(counts.skipped_blueprints as i64)
        .bind(counts.reaction_formulas as i64)
        .bind(counts.reaction_material_lines as i64)
        .bind(counts.reaction_product_lines as i64)
        .bind(counts.skipped_reaction_formulas as i64)
        .bind(counts.categories as i64)
        .bind(counts.groups as i64)
        .bind(counts.meta_groups as i64)
        .bind(counts.market_groups as i64)
        .bind(counts.classified_types as i64)
        .execute(&mut *tx)
        .await
        .map_err(map_sde_sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(SdeError::Storage(format!(
                "SDE import {import_id} is not available for activation"
            )));
        }
        tx.commit().await.map_err(map_sde_sqlx_error)?;
        Ok(())
    }

    async fn fail_import(&self, import_id: Uuid, error: &str) -> Result<(), SdeError> {
        sqlx::query(
            r#"
            UPDATE sde_imports
            SET active = false, status = 'failed', completed_at = now(), error_message = $2
            WHERE id = $1 AND active = false
            "#,
        )
        .bind(import_id)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(())
    }
}
