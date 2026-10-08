use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::{
    resolve_market_price_items, BlueprintKind, BlueprintObservation, BlueprintSelection, Build,
    BuildId, BuildRecipe, BuildRecipeKind, CapturedReactionFormula, CapturedRecipe,
    CapturedRecipeLine, DraftPlanningBatchUpdate, DraftPlanningInput, DraftPlanningSnapshot,
    DraftUpdate, IndustryError, IndustryRepository, MarketPriceRequest, Money, NewBuild, OwnerId,
    PriceSource, PriceSourceId, PriceSourceItem, PriceSourceKind, RecipeCurrency,
    UpdatePriceSourceCommand, WorkspaceId,
};
use iskworks_sde::SdeReadRepository;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{PgMarketRepository, PgSdeRepository};

mod build;
mod canonical;
mod error;
mod pricing;
mod repository_impl;

#[cfg(test)]
mod production_dependency_tests;
#[cfg(test)]
mod tests;

use build::*;
use canonical::*;
use error::*;
use pricing::*;

#[derive(Clone)]
pub struct PgIndustryRepository {
    pool: PgPool,
}

impl PgIndustryRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn load_build(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<Build, IndustryError> {
        let row = sqlx::query_as::<_, BuildRow>(
            r#"
            SELECT id, workspace_id, owner_id, display_name, recipe_kind,
                   blueprint_type_id, blueprint_name, reaction_formula_type_id,
                   reaction_formula_name, duration_seconds_per_run, source_sde_dataset_id,
                   source_sde_version, recipe_fingerprint, runs, notes,
                   revision, created_at, updated_at
            FROM builds
            WHERE workspace_id = $1 AND id = $2
            "#,
        )
        .bind(workspace_id.0)
        .bind(build_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?
        .ok_or(IndustryError::BuildNotFound)?;

        let materials = load_recipe_lines(&self.pool, build_id, "build_recipe_materials").await?;
        let products = load_recipe_lines(&self.pool, build_id, "build_recipe_products").await?;
        let draft_planning = load_draft_planning(&self.pool, build_id).await?;
        let (recipe_currency, active_sde_version) =
            self.compare_recipe(&row, &materials, &products).await;
        let mut build = row.into_build(
            materials,
            products,
            draft_planning,
            recipe_currency,
            active_sde_version,
        )?;
        self.attach_read_model_classification(&mut build).await;
        build.selected_blueprint_origin = self
            .resolve_selected_blueprint_origin(workspace_id, build.draft_planning.as_ref())
            .await;
        build.has_owned_blueprint = self.resolve_has_owned_blueprint(workspace_id, &build).await;
        Ok(build)
    }

    /// Does this workspace hold a blueprint observation for the Build's
    /// blueprint? Manufacturing only -- a reaction Build has no owned
    /// blueprint. Every observation row is current (superseded rows are
    /// deleted on sync), so no `is_current` filter. Best-effort: a query
    /// error reads as "not on hand".
    async fn resolve_has_owned_blueprint(&self, workspace_id: WorkspaceId, build: &Build) -> bool {
        let Some(blueprint_type_id) = build.recipe.blueprint_type_id() else {
            return false;
        };
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM blueprint_observations \
             WHERE workspace_id = $1 AND blueprint_type_id = $2)",
        )
        .bind(workspace_id.0)
        .bind(blueprint_type_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(false)
    }

    /// Best-effort: resolve the output item's SDE category/group for the
    /// Builds library. A missing classification (SDE not imported, product
    /// absent) leaves the fields `None` rather than failing the load.
    async fn attach_read_model_classification(&self, build: &mut Build) {
        let product_type_id = build.recipe.primary_product().type_id;
        let sde = PgSdeRepository::new(self.pool.clone());
        let Ok(map) = sde.type_classifications(&[product_type_id]).await else {
            return;
        };
        if let Some(classification) = map.get(&product_type_id) {
            build
                .product_category_name
                .clone_from(&classification.category_name);
            build
                .product_group_name
                .clone_from(&classification.group_name);
        }
    }

    /// BPO vs BPC for the blueprint actually selected on the Build. For a
    /// manual selection the origin is inline; for an observed asset it's on
    /// the observation row. `None` for no selection, a reaction Build, or a
    /// since-deleted observation.
    async fn resolve_selected_blueprint_origin(
        &self,
        workspace_id: WorkspaceId,
        draft_planning: Option<&DraftPlanningSnapshot>,
    ) -> Option<BlueprintKind> {
        match draft_planning?.input.blueprint_selection.as_ref()? {
            BlueprintSelection::Manual { kind, .. } => Some(*kind),
            // Already captured (the common case): the
            // effective kind is frozen on the selection itself, no live
            // lookup needed at all. Only a not-yet-migrated legacy row
            // (the `Unknown` sentinel) still falls back to a live,
            // best-effort observation lookup.
            BlueprintSelection::ObservedAsset { kind, .. } if *kind != BlueprintKind::Unknown => {
                Some(*kind)
            }
            BlueprintSelection::ObservedAsset { observation_id, .. } => {
                load_blueprint_observation(&self.pool, workspace_id, *observation_id)
                    .await
                    .ok()
                    .map(|observation| observation.kind)
            }
        }
    }

    async fn compare_recipe(
        &self,
        row: &BuildRow,
        materials: &[CapturedRecipeLine],
        products: &[CapturedRecipeLine],
    ) -> (RecipeCurrency, Option<String>) {
        let sde = PgSdeRepository::new(self.pool.clone());
        let Ok(active) = sde.active_sde().await else {
            return (RecipeCurrency::UnableToCompare, None);
        };
        self.compare_recipe_to(active.as_ref(), row, materials, products)
            .await
    }

    /// [`Self::compare_recipe`] against an already-read active SDE, so a
    /// bulk loader reads it once instead of once per Build.
    async fn compare_recipe_to(
        &self,
        active: Option<&iskworks_sde::ActiveSde>,
        row: &BuildRow,
        materials: &[CapturedRecipeLine],
        products: &[CapturedRecipeLine],
    ) -> (RecipeCurrency, Option<String>) {
        let sde = PgSdeRepository::new(self.pool.clone());
        let Some(active) = active else {
            return (RecipeCurrency::UnableToCompare, None);
        };
        if active.import_id == row.source_sde_dataset_id {
            return (RecipeCurrency::Current, Some(active.source_version.clone()));
        }
        let (fingerprint, current_materials, current_products) = match row.recipe_kind.as_str() {
            "manufacturing" => {
                let Some(blueprint_type_id) = row.blueprint_type_id else {
                    return (
                        RecipeCurrency::UnableToCompare,
                        Some(active.source_version.clone()),
                    );
                };
                let Ok(current_recipe) = sde.manufacturing_recipe(blueprint_type_id).await else {
                    return (
                        RecipeCurrency::UnableToCompare,
                        Some(active.source_version.clone()),
                    );
                };
                let Some(current_recipe) = current_recipe else {
                    return (
                        RecipeCurrency::BlueprintNoLongerAvailable,
                        Some(active.source_version.clone()),
                    );
                };
                let Ok(captured) = CapturedRecipe::capture(
                    active.import_id,
                    active.source_version.clone(),
                    current_recipe,
                ) else {
                    return (
                        RecipeCurrency::UnableToCompare,
                        Some(active.source_version.clone()),
                    );
                };
                (captured.fingerprint, captured.materials, captured.products)
            }
            "reaction" => {
                let Some(reaction_formula_type_id) = row.reaction_formula_type_id else {
                    return (
                        RecipeCurrency::UnableToCompare,
                        Some(active.source_version.clone()),
                    );
                };
                let Ok(current_formula) = sde.reaction_formula(reaction_formula_type_id).await
                else {
                    return (
                        RecipeCurrency::UnableToCompare,
                        Some(active.source_version.clone()),
                    );
                };
                let Some(current_formula) = current_formula else {
                    return (
                        RecipeCurrency::ReactionFormulaNoLongerAvailable,
                        Some(active.source_version.clone()),
                    );
                };
                let Ok(captured) = CapturedReactionFormula::capture(
                    active.import_id,
                    active.source_version.clone(),
                    current_formula,
                ) else {
                    return (
                        RecipeCurrency::UnableToCompare,
                        Some(active.source_version.clone()),
                    );
                };
                (captured.fingerprint, captured.materials, captured.products)
            }
            _ => {
                return (
                    RecipeCurrency::UnableToCompare,
                    Some(active.source_version.clone()),
                )
            }
        };
        let state = if fingerprint == row.recipe_fingerprint
            && current_materials == materials
            && current_products == products
        {
            RecipeCurrency::OlderSdeVersion
        } else {
            RecipeCurrency::RecipeChanged
        };
        (state, Some(active.source_version.clone()))
    }

    async fn load_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<PriceSource, IndustryError> {
        let row = sqlx::query_as::<_, PriceSourceRow>(
            r#"
            SELECT ps.id, ps.workspace_id, ps.display_name, ps.description, ps.source_kind,
                   ps.revision, ps.created_at, ps.updated_at,
                   CASE
                     WHEN ps.source_kind = 'eve_client_market_export' THEN (
                       SELECT COUNT(DISTINCT mif.type_id)::bigint
                       FROM market_import_files mif
                       JOIN market_price_source_configs config ON config.price_source_id = ps.id
                       WHERE mif.workspace_id = ps.workspace_id
                         AND mif.location_id = config.location_id
                         AND (
                           config.observation_mode = 'latest_compatible_import'
                           OR mif.batch_id = config.pinned_batch_id
                         )
                     )
                     WHEN ps.source_kind = 'esi_market_orders' THEN (
                       SELECT COUNT(DISTINCT coverage.type_id)::bigint
                       FROM market_source_coverage coverage
                       WHERE coverage.price_source_id = ps.id
                     )
                     ELSE (
                       SELECT COUNT(DISTINCT psi.type_id)::bigint
                       FROM price_source_items psi
                       WHERE psi.price_source_id = ps.id
                     )
                   END AS item_count,
                   (
                     SELECT COUNT(DISTINCT o.source_build_id)::bigint
                     FROM price_snapshots snap
                     JOIN orders o ON o.price_snapshot_id = snap.id
                     WHERE snap.price_source_id = ps.id
                   ) AS recent_build_count
            FROM price_sources ps
            WHERE ps.workspace_id = $1 AND ps.id = $2
            "#,
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?
        .ok_or(IndustryError::PriceSourceNotFound)?;
        let items = sqlx::query_as::<_, PriceSourceItemRow>(
            r#"
            SELECT type_id, captured_name, price, note, updated_at
            FROM price_source_items
            WHERE price_source_id = $1
            ORDER BY lower(captured_name), type_id
            "#,
        )
        .bind(source_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(PriceSourceItemRow::into_item)
        .collect();
        row.into_source(items)
    }
}
