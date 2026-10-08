use super::sde_import::PgSdeRepository;
use super::*;

/// Security classification shared by every query in this crate that needs
/// it (`search_solar_systems`/`search_npc_stations`/
/// `list_npc_stations_in_region` below, plus
/// `PgFacilityRepository::search_known_structures`/`get_known_structure` in
/// `facility.rs`). Expects a `system` alias for `sde_solar_systems` in
/// scope.
///
/// `wormhole_class_id` is neither necessary nor sufficient for "this system
/// is in wormhole/Anoikis space" in our imported SDE: verified live, it is
/// `NULL` on the large majority of real Anoikis systems (2,599 of 2,604),
/// and non-null (class 8, EVE's "shattered wormhole" *environmental
/// effect*, not a space classification) on 687 ordinary k-space systems,
/// including highsec systems in The Forge. Using `wormhole_class_id IS NOT
/// NULL` therefore both mislabels hundreds of k-space systems as wormhole
/// space *and* fails to label most real wormhole systems as such (they fall
/// through to the `security_status`-based `'nullSec'` branch instead).
///
/// Anoikis is reliably identified by region membership instead:
/// `region_id` 11000001..=11000033 is exactly, and only, EVE's 33 wormhole
/// regions (verified against this SDE's `sde_regions` table --
/// min/max/count over that range is 11000001/11000033/33, and no k-space
/// region's `region_id` falls inside it). This range is EVE's own static
/// data convention, not something specific to one SDE import, so it's
/// centralized here rather than hardcoded per query.
pub(crate) const SECURITY_CLASS_CASE_SQL: &str = "CASE
                        WHEN system.region_id BETWEEN 11000001 AND 11000033 THEN 'wormhole'
                        WHEN system.security_status >= 0.45 THEN 'highSec'
                        WHEN system.security_status > 0 THEN 'lowSec'
                        WHEN system.security_status IS NOT NULL THEN 'nullSec'
                        ELSE 'unknown'
                      END";

/// The market-relevant `region_id` range -- see `list_regions`' own doc
/// comment for the live-data investigation behind this exact boundary.
/// Centralized so `search_regions` can't drift from `list_regions` by
/// duplicating the numeric range separately. Expects an `r`
/// alias for `sde_regions` in scope.
pub(crate) const MARKET_RELEVANT_REGION_FILTER_SQL: &str =
    "r.region_id BETWEEN 10000001 AND 10000069";

#[async_trait]
impl SdeReadRepository for PgSdeRepository {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        let row = sqlx::query_as::<_, ActiveImportRow>(
            r#"
            SELECT id, source_version, source_label, source_checksum, completed_at,
                   type_count, blueprint_count, material_line_count, product_line_count,
                   skipped_blueprint_count, reaction_formula_count,
                   reaction_material_line_count, reaction_product_line_count,
                   skipped_reaction_formula_count, category_count, group_count,
                   meta_group_count, market_group_count, classified_type_count
            FROM sde_imports
            WHERE active = true
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        Ok(row.map(|row| ActiveSde {
            import_id: row.id,
            source_version: row.source_version,
            source_label: row.source_label,
            source_checksum: row.source_checksum,
            completed_at: row.completed_at,
            counts: ImportCounts {
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
        }))
    }

    /// Discovers every matching manufacturing and reaction recipe in three
    /// SQL statements regardless of candidate count: headers, materials,
    /// then products. Recipe lines are assembled in memory.
    async fn manufacturable_candidates(
        &self,
        scope: &ManufacturableCandidateScope,
    ) -> Result<Vec<ManufacturableCandidateRecipe>, SdeError> {
        let kinds = scope
            .recipe_kinds
            .iter()
            .map(|kind| match kind {
                CandidateRecipeKind::Manufacturing => "manufacturing",
                CandidateRecipeKind::Reaction => "reaction",
            })
            .collect::<Vec<_>>();
        let category_ids = scope.category_ids.iter().copied().collect::<Vec<_>>();
        let group_ids = scope.group_ids.iter().copied().collect::<Vec<_>>();
        let meta_group_ids = scope.meta_group_ids.iter().copied().collect::<Vec<_>>();
        let market_roots = scope
            .market_group_root_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let headers = sqlx::query_as::<_, CandidateHeaderRow>(r#"
            WITH RECURSIVE market_tree AS (
              SELECT import_id, market_group_id FROM sde_market_groups
              WHERE market_group_id = ANY($5)
              UNION ALL
              SELECT child.import_id, child.market_group_id
              FROM sde_market_groups child JOIN market_tree parent
                ON child.import_id = parent.import_id AND child.parent_group_id = parent.market_group_id
            ), market_ancestors AS (
              SELECT import_id, market_group_id leaf_market_group_id,
                     market_group_id, name_en, parent_group_id, 0 depth
              FROM sde_market_groups
              UNION ALL
              SELECT child.import_id, child.leaf_market_group_id,
                     parent.market_group_id, parent.name_en, parent.parent_group_id,
                     child.depth + 1
              FROM market_ancestors child
              JOIN sde_market_groups parent
                ON parent.import_id = child.import_id
               AND parent.market_group_id = child.parent_group_id
            ), recipes AS (
              SELECT 'manufacturing'::text kind, i.id import_id, i.source_version,
                     b.blueprint_type_id recipe_id, b.name_en recipe_name, b.duration_seconds,
                     p.product_type_id, p.position,
                     count(*) OVER (PARTITION BY b.import_id, b.blueprint_type_id) product_count
              FROM sde_imports i JOIN sde_blueprints b ON b.import_id=i.id
              JOIN sde_blueprint_products p ON p.import_id=b.import_id AND p.blueprint_type_id=b.blueprint_type_id
              WHERE i.active
              UNION ALL
              SELECT 'reaction', i.id, i.source_version, f.reaction_formula_type_id,
                     f.name_en, f.duration_seconds, p.product_type_id, p.position,
                     count(*) OVER (PARTITION BY f.import_id, f.reaction_formula_type_id)
              FROM sde_imports i JOIN sde_reaction_formulas f ON f.import_id=i.id
              JOIN sde_reaction_formula_products p ON p.import_id=f.import_id AND p.reaction_formula_type_id=f.reaction_formula_type_id
              WHERE i.active
            )
            SELECT r.kind, r.import_id, r.source_version, r.recipe_id, r.recipe_name,
                   r.duration_seconds, r.product_type_id, r.product_count,
                   product.published product_published, recipe_type.published recipe_published,
                   product.group_id, groups.name_en group_name, groups.category_id,
                   categories.name_en category_name, product.meta_group_id,
                   meta.name_en meta_group_name, product.market_group_id,
                   market.name_en market_group_name,
                   ARRAY(
                     SELECT ancestor.market_group_id
                     FROM market_ancestors ancestor
                     WHERE ancestor.import_id = product.import_id
                       AND ancestor.leaf_market_group_id = product.market_group_id
                     ORDER BY ancestor.depth DESC
                   ) market_group_ancestor_ids,
                   ARRAY(
                     SELECT ancestor.name_en
                     FROM market_ancestors ancestor
                     WHERE ancestor.import_id = product.import_id
                       AND ancestor.leaf_market_group_id = product.market_group_id
                     ORDER BY ancestor.depth DESC
                   ) market_group_ancestor_names
            FROM recipes r
            JOIN sde_types product ON product.import_id=r.import_id AND product.type_id=r.product_type_id
            JOIN sde_types recipe_type ON recipe_type.import_id=r.import_id AND recipe_type.type_id=r.recipe_id
            LEFT JOIN sde_groups groups ON groups.import_id=product.import_id AND groups.group_id=product.group_id
            LEFT JOIN sde_categories categories ON categories.import_id=groups.import_id AND categories.category_id=groups.category_id
            LEFT JOIN sde_meta_groups meta ON meta.import_id=product.import_id AND meta.meta_group_id=product.meta_group_id
            LEFT JOIN sde_market_groups market ON market.import_id=product.import_id AND market.market_group_id=product.market_group_id
            WHERE r.position = 0 AND product.published AND recipe_type.published
              AND (cardinality($1::text[]) = 0 OR r.kind = ANY($1))
              AND (cardinality($2::bigint[]) = 0 OR groups.category_id = ANY($2))
              AND (cardinality($3::bigint[]) = 0 OR product.group_id = ANY($3))
              AND (cardinality($4::bigint[]) = 0 OR product.meta_group_id = ANY($4))
              AND (cardinality($5::bigint[]) = 0 OR EXISTS (
                    SELECT 1 FROM market_tree tree WHERE tree.import_id=product.import_id AND tree.market_group_id=product.market_group_id))
            ORDER BY product.name_en, r.kind, r.recipe_id
        "#)
        .bind(&kinds).bind(&category_ids).bind(&group_ids).bind(&meta_group_ids).bind(&market_roots)
        .fetch_all(&self.pool).await.map_err(map_sde_sqlx_error)?;
        let manufacturing_ids = headers
            .iter()
            .filter(|row| row.kind == "manufacturing")
            .map(|row| row.recipe_id)
            .collect::<Vec<_>>();
        let reaction_ids = headers
            .iter()
            .filter(|row| row.kind == "reaction")
            .map(|row| row.recipe_id)
            .collect::<Vec<_>>();
        let materials =
            candidate_lines(&self.pool, &manufacturing_ids, &reaction_ids, true).await?;
        let products =
            candidate_lines(&self.pool, &manufacturing_ids, &reaction_ids, false).await?;
        Ok(headers
            .into_iter()
            .map(|row| {
                let identity = if row.kind == "manufacturing" {
                    CandidateRecipeIdentity::Manufacturing {
                        blueprint_type_id: row.recipe_id,
                    }
                } else {
                    CandidateRecipeIdentity::Reaction {
                        reaction_formula_type_id: row.recipe_id,
                    }
                };
                let key = (row.kind.clone(), row.recipe_id);
                ManufacturableCandidateRecipe {
                    import_id: row.import_id,
                    source_version: row.source_version,
                    identity,
                    recipe_name: row.recipe_name,
                    duration_seconds: row.duration_seconds,
                    materials: materials.get(&key).cloned().unwrap_or_default(),
                    products: products.get(&key).cloned().unwrap_or_default(),
                    primary_product_type_id: row.product_type_id,
                    primary_product_published: row.product_published,
                    recipe_type_published: row.recipe_published,
                    classification: CandidateProductClassification {
                        category_id: row.category_id,
                        category_name: row.category_name,
                        group_id: row.group_id,
                        group_name: row.group_name,
                        meta_group_id: row.meta_group_id,
                        meta_group_name: row.meta_group_name,
                        market_group_id: row.market_group_id,
                        market_group_name: row.market_group_name,
                        market_group_ancestry: row
                            .market_group_ancestor_ids
                            .into_iter()
                            .zip(row.market_group_ancestor_names)
                            .map(|(market_group_id, name)| CandidateMarketGroup {
                                market_group_id,
                                name,
                            })
                            .collect(),
                    },
                    has_additional_products: row.product_count > 1,
                }
            })
            .collect())
    }

    async fn search_manufacturing_blueprints(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let prefix = format!("{query}%");
        let contains = format!("%{query}%");
        let rows = sqlx::query_as::<_, BlueprintSearchRow>(
            r#"
            SELECT b.blueprint_type_id,
                   b.name_en AS blueprint_name,
                   p.product_type_id,
                   product.name_en AS product_name,
                   product.group_name_en AS group_name,
                   product.published
            FROM sde_imports import
            JOIN sde_blueprints b ON b.import_id = import.id
            JOIN sde_blueprint_products p
              ON p.import_id = b.import_id
             AND p.blueprint_type_id = b.blueprint_type_id
            JOIN sde_types product
              ON product.import_id = p.import_id
             AND product.type_id = p.product_type_id
            WHERE import.active = true
              AND (
                lower(b.name_en) LIKE lower($1)
                OR lower(product.name_en) LIKE lower($1)
                OR lower(b.name_en) LIKE lower($2)
                OR lower(product.name_en) LIKE lower($2)
              )
            ORDER BY
              CASE
                WHEN lower(product.name_en) = lower($3) THEN 0
                WHEN lower(b.name_en) = lower($3) THEN 1
                WHEN lower(product.name_en) LIKE lower($1) THEN 2
                ELSE 3
              END,
              product.name_en,
              b.blueprint_type_id
            LIMIT $4
            "#,
        )
        .bind(prefix)
        .bind(contains)
        .bind(query)
        .bind(i64::from(limit.clamp(1, 50)))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|row| BlueprintSearchResult {
                blueprint_type_id: row.blueprint_type_id,
                blueprint_name: row.blueprint_name,
                product_type_id: row.product_type_id,
                product_name: row.product_name,
                group_name: row.group_name,
                published: row.published,
                manufacturing_available: true,
            })
            .collect())
    }

    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        let blueprint = sqlx::query_as::<_, RecipeHeaderRow>(
            r#"
            SELECT b.blueprint_type_id, b.name_en, b.duration_seconds
            FROM sde_imports import
            JOIN sde_blueprints b ON b.import_id = import.id
            WHERE import.active = true AND b.blueprint_type_id = $1
            "#,
        )
        .bind(blueprint_type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        let Some(blueprint) = blueprint else {
            return Ok(None);
        };

        let materials = sqlx::query_as::<_, RecipeLineRow>(
            r#"
            SELECT line.material_type_id AS type_id, item.name_en AS type_name, line.quantity
            FROM sde_imports import
            JOIN sde_blueprint_materials line ON line.import_id = import.id
            JOIN sde_types item
              ON item.import_id = line.import_id
             AND item.type_id = line.material_type_id
            WHERE import.active = true AND line.blueprint_type_id = $1
            ORDER BY line.position, line.material_type_id
            "#,
        )
        .bind(blueprint_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        let products = sqlx::query_as::<_, RecipeLineRow>(
            r#"
            SELECT line.product_type_id AS type_id, item.name_en AS type_name, line.quantity
            FROM sde_imports import
            JOIN sde_blueprint_products line ON line.import_id = import.id
            JOIN sde_types item
              ON item.import_id = line.import_id
             AND item.type_id = line.product_type_id
            WHERE import.active = true AND line.blueprint_type_id = $1
            ORDER BY line.position, line.product_type_id
            "#,
        )
        .bind(blueprint_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        Ok(Some(ManufacturingRecipe {
            blueprint_type_id: blueprint.blueprint_type_id,
            blueprint_name: blueprint.name_en,
            duration_seconds: blueprint.duration_seconds,
            materials: materials
                .into_iter()
                .map(RecipeLineRow::into_line)
                .collect(),
            products: products.into_iter().map(RecipeLineRow::into_line).collect(),
        }))
    }

    async fn search_reaction_formulas(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<ReactionFormulaSearchResult>, SdeError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let prefix = format!("{query}%");
        let contains = format!("%{query}%");
        let rows = sqlx::query_as::<_, ReactionFormulaSearchRow>(
            r#"
            SELECT f.reaction_formula_type_id,
                   f.name_en AS reaction_formula_name,
                   p.product_type_id,
                   product.name_en AS product_name,
                   product.group_name_en AS group_name,
                   product.published
            FROM sde_imports import
            JOIN sde_reaction_formulas f ON f.import_id = import.id
            JOIN sde_reaction_formula_products p
              ON p.import_id = f.import_id
             AND p.reaction_formula_type_id = f.reaction_formula_type_id
            JOIN sde_types product
              ON product.import_id = p.import_id
             AND product.type_id = p.product_type_id
            WHERE import.active = true
              AND (
                lower(f.name_en) LIKE lower($1)
                OR lower(product.name_en) LIKE lower($1)
                OR lower(f.name_en) LIKE lower($2)
                OR lower(product.name_en) LIKE lower($2)
              )
            ORDER BY
              CASE
                WHEN lower(product.name_en) = lower($3) THEN 0
                WHEN lower(f.name_en) = lower($3) THEN 1
                WHEN lower(product.name_en) LIKE lower($1) THEN 2
                ELSE 3
              END,
              product.name_en,
              f.reaction_formula_type_id
            LIMIT $4
            "#,
        )
        .bind(prefix)
        .bind(contains)
        .bind(query)
        .bind(i64::from(limit.clamp(1, 50)))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|row| ReactionFormulaSearchResult {
                reaction_formula_type_id: row.reaction_formula_type_id,
                reaction_formula_name: row.reaction_formula_name,
                product_type_id: row.product_type_id,
                product_name: row.product_name,
                group_name: row.group_name,
                published: row.published,
            })
            .collect())
    }

    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        let formula = sqlx::query_as::<_, ReactionFormulaHeaderRow>(
            r#"
            SELECT f.reaction_formula_type_id, f.name_en, f.duration_seconds
            FROM sde_imports import
            JOIN sde_reaction_formulas f ON f.import_id = import.id
            WHERE import.active = true AND f.reaction_formula_type_id = $1
            "#,
        )
        .bind(reaction_formula_type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        let Some(formula) = formula else {
            return Ok(None);
        };

        let materials = sqlx::query_as::<_, RecipeLineRow>(
            r#"
            SELECT line.material_type_id AS type_id, item.name_en AS type_name, line.quantity
            FROM sde_imports import
            JOIN sde_reaction_formula_materials line ON line.import_id = import.id
            JOIN sde_types item
              ON item.import_id = line.import_id
             AND item.type_id = line.material_type_id
            WHERE import.active = true AND line.reaction_formula_type_id = $1
            ORDER BY line.position, line.material_type_id
            "#,
        )
        .bind(reaction_formula_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        let products = sqlx::query_as::<_, RecipeLineRow>(
            r#"
            SELECT line.product_type_id AS type_id, item.name_en AS type_name, line.quantity
            FROM sde_imports import
            JOIN sde_reaction_formula_products line ON line.import_id = import.id
            JOIN sde_types item
              ON item.import_id = line.import_id
             AND item.type_id = line.product_type_id
            WHERE import.active = true AND line.reaction_formula_type_id = $1
            ORDER BY line.position, line.product_type_id
            "#,
        )
        .bind(reaction_formula_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        Ok(Some(ReactionFormulaRecipe {
            reaction_formula_type_id: formula.reaction_formula_type_id,
            reaction_formula_name: formula.name_en,
            duration_seconds: formula.duration_seconds,
            materials: materials
                .into_iter()
                .map(RecipeLineRow::into_line)
                .collect(),
            products: products.into_iter().map(RecipeLineRow::into_line).collect(),
        }))
    }

    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        // Only *published* blueprints are real, in-game recipes. The SDE
        // also ships unpublished CCP test/debug blueprints that can share a
        // product type with the genuine one (e.g. a "Test ... Blueprint"
        // with nonsense material/product quantities); without this filter a
        // bare `LIMIT 1` with no ordering could return that instead. Order
        // deterministically so a genuine tie (two published blueprints for
        // one product) is at least stable across calls. Mirrors the
        // `product.published AND recipe_type.published` guard the forward
        // `search_manufacturing_blueprints` query already applies.
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT p.blueprint_type_id
            FROM sde_imports import
            JOIN sde_blueprint_products p ON p.import_id = import.id
            JOIN sde_types recipe_type
              ON recipe_type.import_id = import.id
             AND recipe_type.type_id = p.blueprint_type_id
            WHERE import.active = true
              AND p.product_type_id = $1
              AND recipe_type.published = true
            ORDER BY p.blueprint_type_id
            LIMIT 1
            "#,
        )
        .bind(product_type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)
    }

    async fn reaction_formula_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        // Same rationale as `manufacturing_blueprint_for_product`: filter to
        // published reaction formulas so an unpublished CCP "Test Reaction
        // Blueprint" (which can share a product type with the genuine
        // formula and carry a different output-per-run) is never resolved
        // as the buildable recipe, and order deterministically.
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT p.reaction_formula_type_id
            FROM sde_imports import
            JOIN sde_reaction_formula_products p ON p.import_id = import.id
            JOIN sde_types recipe_type
              ON recipe_type.import_id = import.id
             AND recipe_type.type_id = p.reaction_formula_type_id
            WHERE import.active = true
              AND p.product_type_id = $1
              AND recipe_type.published = true
            ORDER BY p.reaction_formula_type_id
            LIMIT 1
            "#,
        )
        .bind(product_type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)
    }

    async fn production_recipes_for_products(
        &self,
        product_type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, iskworks_sde::ProductRecipeRefs>, SdeError> {
        if product_type_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        // One query for both recipe kinds, with exactly the single-type
        // lookups' semantics: published recipes only, lowest id wins.
        let rows = sqlx::query_as::<_, (i64, Option<i64>, Option<i64>)>(
            r#"
            WITH active AS (SELECT id FROM sde_imports WHERE active = true),
            blueprints AS (
              SELECT p.product_type_id, min(p.blueprint_type_id) AS recipe_type_id
              FROM sde_blueprint_products p
              JOIN active ON active.id = p.import_id
              JOIN sde_types recipe_type
                ON recipe_type.import_id = p.import_id
               AND recipe_type.type_id = p.blueprint_type_id
              WHERE p.product_type_id = ANY($1) AND recipe_type.published = true
              GROUP BY p.product_type_id
            ),
            formulas AS (
              SELECT p.product_type_id, min(p.reaction_formula_type_id) AS recipe_type_id
              FROM sde_reaction_formula_products p
              JOIN active ON active.id = p.import_id
              JOIN sde_types recipe_type
                ON recipe_type.import_id = p.import_id
               AND recipe_type.type_id = p.reaction_formula_type_id
              WHERE p.product_type_id = ANY($1) AND recipe_type.published = true
              GROUP BY p.product_type_id
            )
            SELECT coalesce(b.product_type_id, f.product_type_id),
                   b.recipe_type_id, f.recipe_type_id
            FROM blueprints b
            FULL OUTER JOIN formulas f ON f.product_type_id = b.product_type_id
            "#,
        )
        .bind(product_type_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|(type_id, blueprint_type_id, reaction_formula_type_id)| {
                (
                    type_id,
                    iskworks_sde::ProductRecipeRefs {
                        blueprint_type_id,
                        reaction_formula_type_id,
                    },
                )
            })
            .collect())
    }

    async fn search_types(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let contains = format!("%{query}%");
        sqlx::query_as::<_, TypeSearchRow>(
            r#"
            SELECT t.type_id, t.name_en AS type_name, t.group_name_en AS group_name, t.published
            FROM sde_imports import
            JOIN sde_types t ON t.import_id = import.id
            WHERE import.active = true AND lower(t.name_en) LIKE lower($1)
            ORDER BY
              CASE WHEN lower(t.name_en) = lower($2) THEN 0 ELSE 1 END,
              t.name_en, t.type_id
            LIMIT $3
            "#,
        )
        .bind(contains)
        .bind(query)
        .bind(i64::from(limit.clamp(1, 50)))
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.into_iter().map(TypeSearchRow::into_result).collect())
        .map_err(map_sde_sqlx_error)
    }

    async fn type_group_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        if type_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let rows = sqlx::query_as::<_, TypeGroupNameRow>(
            r#"
            SELECT t.type_id, t.group_name_en
            FROM sde_imports import
            JOIN sde_types t ON t.import_id = import.id
            WHERE import.active = true AND t.type_id = ANY($1)
            "#,
        )
        .bind(type_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .filter_map(|row| row.group_name_en.map(|name| (row.type_id, name)))
            .collect())
    }

    async fn type_classifications(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, SdeTypeClassification>, SdeError> {
        if type_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let rows = sqlx::query_as::<_, TypeClassificationRow>(
            r#"
            SELECT t.type_id,
                   t.group_id AS group_id,
                   g.name_en AS group_name,
                   g.category_id AS category_id,
                   c.name_en AS category_name
            FROM sde_imports import
            JOIN sde_types t ON t.import_id = import.id
            LEFT JOIN sde_groups g ON g.import_id = import.id AND g.group_id = t.group_id
            LEFT JOIN sde_categories c ON c.import_id = import.id AND c.category_id = g.category_id
            WHERE import.active = true AND t.type_id = ANY($1)
            "#,
        )
        .bind(type_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.type_id,
                    SdeTypeClassification {
                        category_id: row.category_id,
                        category_name: row.category_name,
                        group_id: row.group_id,
                        group_name: row.group_name,
                    },
                )
            })
            .collect())
    }

    async fn type_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        if type_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let rows = sqlx::query_as::<_, TypeNameRow>(
            r#"
            SELECT t.type_id, t.name_en
            FROM sde_imports import
            JOIN sde_types t ON t.import_id = import.id
            WHERE import.active = true AND t.type_id = ANY($1)
            "#,
        )
        .bind(type_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|row| (row.type_id, row.name_en))
            .collect())
    }

    async fn inventory_type_metadata(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, SdeInventoryTypeMetadata>, SdeError> {
        if type_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let rows = sqlx::query_as::<_, InventoryTypeMetadataRow>(
            r#"
            SELECT t.type_id, t.group_name_en, t.packaged_volume_m3
            FROM sde_imports import
            JOIN sde_types t ON t.import_id = import.id
            WHERE import.active = true AND t.type_id = ANY($1)
            "#,
        )
        .bind(type_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.type_id,
                    SdeInventoryTypeMetadata {
                        group_name: row.group_name_en,
                        packaged_volume_m3: row.packaged_volume_m3,
                    },
                )
            })
            .collect())
    }

    async fn type_reference(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, iskworks_sde::TypeReference>, SdeError> {
        if type_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let rows = sqlx::query_as::<_, TypeReferenceRow>(
            r#"
            SELECT t.type_id,
                   t.name_en           AS type_name,
                   t.group_id          AS group_id,
                   g.name_en           AS group_name,
                   g.category_id       AS category_id,
                   c.name_en           AS category_name,
                   t.packaged_volume_m3 AS packaged_volume_m3
            FROM sde_imports import
            JOIN sde_types t ON t.import_id = import.id
            LEFT JOIN sde_groups g ON g.import_id = import.id AND g.group_id = t.group_id
            LEFT JOIN sde_categories c ON c.import_id = import.id AND c.category_id = g.category_id
            WHERE import.active = true AND t.type_id = ANY($1)
            "#,
        )
        .bind(type_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.type_id,
                    iskworks_sde::TypeReference {
                        type_name: Some(row.type_name),
                        group_id: row.group_id,
                        group_name: row.group_name,
                        category_id: row.category_id,
                        category_name: row.category_name,
                        packaged_volume_m3: row.packaged_volume_m3,
                    },
                )
            })
            .collect())
    }

    async fn search_structure_types(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        const STRUCTURE_GROUP_IDS: [i64; 8] = [1404, 1406, 1408, 1657, 2015, 2016, 2017, 4744];

        let query = query.trim();
        let contains = format!("%{query}%");
        sqlx::query_as::<_, TypeSearchRow>(
            r#"
            SELECT t.type_id, t.name_en AS type_name, t.group_name_en AS group_name, t.published
            FROM sde_imports import
            JOIN sde_types t ON t.import_id = import.id
            WHERE import.active = true
              AND t.published = true
              AND t.group_id = ANY($1)
              AND lower(t.name_en) LIKE lower($2)
            ORDER BY
              CASE WHEN lower(t.name_en) = lower($3) THEN 0 ELSE 1 END,
              t.name_en, t.type_id
            LIMIT $4
            "#,
        )
        .bind(STRUCTURE_GROUP_IDS.as_slice())
        .bind(contains)
        .bind(query)
        .bind(i64::from(limit.clamp(1, 250)))
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.into_iter().map(TypeSearchRow::into_result).collect())
        .map_err(map_sde_sqlx_error)
    }

    async fn search_structure_rigs(
        &self,
        query: &str,
        limit: u32,
        structure_type_id: Option<i64>,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let query = query.trim();
        let contains = format!("%{query}%");
        sqlx::query_as::<_, TypeSearchRow>(
            r#"
            SELECT t.type_id, t.name_en AS type_name, t.group_name_en AS group_name, t.published
            FROM sde_imports import
            JOIN sde_types t ON t.import_id = import.id
            JOIN sde_structure_rig_modifiers rig
              ON rig.import_id=import.id AND rig.type_id=t.type_id
            LEFT JOIN sde_types structure_type
              ON structure_type.import_id=import.id AND structure_type.type_id=$3
            LEFT JOIN sde_structure_manufacturing_modifiers structure
              ON structure.import_id=import.id AND structure.type_id=structure_type.type_id
            WHERE import.active = true
              AND t.published = true
              AND t.group_name_en ILIKE 'Structure%Rig%'
              AND t.group_id <> 1708
              AND lower(t.name_en) LIKE lower($1)
              AND (
                $3::bigint IS NULL
                OR (
                  structure_type.group_id = ANY(rig.compatible_structure_group_ids)
                  AND structure.structure_size = rig.rig_size
                )
              )
            ORDER BY
              CASE WHEN lower(t.name_en) = lower($2) THEN 0 ELSE 1 END,
              t.name_en, t.type_id
            LIMIT $3
            "#,
        )
        .bind(contains)
        .bind(query)
        .bind(structure_type_id)
        .bind(i64::from(limit.clamp(1, 250)))
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.into_iter().map(TypeSearchRow::into_result).collect())
        .map_err(map_sde_sqlx_error)
    }

    async fn structure_manufacturing_modifiers(
        &self,
        type_id: i64,
    ) -> Result<Option<iskworks_sde::StructureManufacturingModifiers>, SdeError> {
        let row = sqlx::query_as::<_, StructureModifierRow>(
            r#"SELECT structure_type.type_id,
                      COALESCE(modifiers.material_reduction_percent, 0) AS material_reduction_percent,
                      COALESCE(modifiers.time_reduction_percent, 0) AS time_reduction_percent,
                      COALESCE(modifiers.job_cost_reduction_percent, 0) AS job_cost_reduction_percent
               FROM sde_imports import
               JOIN sde_types structure_type
                 ON structure_type.import_id=import.id AND structure_type.type_id=$1
               LEFT JOIN sde_structure_manufacturing_modifiers modifiers
                 ON modifiers.import_id=import.id AND modifiers.type_id=structure_type.type_id
               WHERE import.active=true"#,
        )
        .bind(type_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(
            row.map(|row| iskworks_sde::StructureManufacturingModifiers {
                type_id: row.type_id,
                material_reduction_percent: row.material_reduction_percent.normalize().to_string(),
                time_reduction_percent: row.time_reduction_percent.normalize().to_string(),
                job_cost_reduction_percent: row.job_cost_reduction_percent.normalize().to_string(),
            }),
        )
    }

    async fn rig_manufacturing_modifiers(
        &self,
        type_id: i64,
        security_class: &str,
        structure_type_id: Option<i64>,
    ) -> Result<Option<iskworks_sde::RigManufacturingModifiers>, SdeError> {
        let multiplier_expression = match security_class {
            "highSec" => "rig.high_sec_multiplier",
            "lowSec" => "rig.low_sec_multiplier",
            "nullSec" | "wormhole" => "rig.null_sec_multiplier",
            _ => "1",
        };
        let query = rig_modifier_query("sde_structure_rig_modifiers", multiplier_expression);
        let row = sqlx::query_as::<_, RigModifierRow>(&query)
            .bind(type_id)
            .bind(structure_type_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sde_sqlx_error)?;
        Ok(row.map(|row| iskworks_sde::RigManufacturingModifiers {
            type_id: row.type_id,
            material_reduction_percent: row.material_reduction_percent.normalize().to_string(),
            time_reduction_percent: row.time_reduction_percent.normalize().to_string(),
            compatible_with_structure: row.compatible_with_structure,
            applies_to: row.applicability(),
        }))
    }

    async fn search_reaction_rigs(
        &self,
        query: &str,
        limit: u32,
        structure_type_id: Option<i64>,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let query = query.trim();
        let contains = format!("%{query}%");
        sqlx::query_as::<_, TypeSearchRow>(
            r#"
            SELECT t.type_id, t.name_en AS type_name, t.group_name_en AS group_name, t.published
            FROM sde_imports import
            JOIN sde_types t ON t.import_id = import.id
            JOIN sde_reaction_rig_modifiers rig
              ON rig.import_id=import.id AND rig.type_id=t.type_id
            LEFT JOIN sde_types structure_type
              ON structure_type.import_id=import.id AND structure_type.type_id=$3
            LEFT JOIN sde_structure_manufacturing_modifiers structure
              ON structure.import_id=import.id AND structure.type_id=structure_type.type_id
            WHERE import.active = true
              AND t.published = true
              AND lower(t.name_en) LIKE lower($1)
              AND (
                $3::bigint IS NULL
                OR (
                  structure_type.group_id = ANY(rig.compatible_structure_group_ids)
                  AND structure.structure_size = rig.rig_size
                )
              )
            ORDER BY
              CASE WHEN lower(t.name_en) = lower($2) THEN 0 ELSE 1 END,
              t.name_en, t.type_id
            LIMIT $4
            "#,
        )
        .bind(contains)
        .bind(query)
        .bind(structure_type_id)
        .bind(i64::from(limit.clamp(1, 250)))
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.into_iter().map(TypeSearchRow::into_result).collect())
        .map_err(map_sde_sqlx_error)
    }

    async fn planet_schematics(
        &self,
        schematic_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, iskworks_sde::PlanetSchematic>, SdeError> {
        if schematic_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let rows = sqlx::query_as::<_, PlanetSchematicLineRow>(
            r#"
            SELECT schematic.schematic_id, schematic.name_en, schematic.cycle_time_seconds,
                   line.type_id, line.is_input, line.quantity
            FROM sde_imports import
            JOIN sde_planet_schematics schematic ON schematic.import_id = import.id
            JOIN sde_planet_schematic_types line
              ON line.import_id = schematic.import_id
             AND line.schematic_id = schematic.schematic_id
            WHERE import.active = true AND schematic.schematic_id = ANY($1)
            ORDER BY schematic.schematic_id, line.is_input DESC, line.type_id
            "#,
        )
        .bind(schematic_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        let mut schematics = std::collections::BTreeMap::new();
        for row in rows {
            let schematic = schematics.entry(row.schematic_id).or_insert_with(|| {
                iskworks_sde::PlanetSchematic {
                    schematic_id: row.schematic_id,
                    name: row.name_en.clone(),
                    cycle_time_seconds: row.cycle_time_seconds,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                }
            });
            let line = iskworks_sde::PlanetSchematicLine {
                type_id: row.type_id,
                quantity: row.quantity,
            };
            if row.is_input {
                schematic.inputs.push(line);
            } else {
                schematic.outputs.push(line);
            }
        }
        Ok(schematics)
    }

    async fn planet_references(
        &self,
        planet_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, iskworks_sde::PlanetReference>, SdeError> {
        if planet_ids.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let rows = sqlx::query_as::<_, PlanetReferenceRow>(
            r#"
            SELECT planet.planet_id, planet.name_en, planet.solar_system_id,
                   system.name_en AS solar_system_name, system.security_status
            FROM sde_imports import
            JOIN sde_planets planet ON planet.import_id = import.id
            JOIN sde_solar_systems system
              ON system.import_id = planet.import_id
             AND system.solar_system_id = planet.solar_system_id
            WHERE import.active = true AND planet.planet_id = ANY($1)
            "#,
        )
        .bind(planet_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.planet_id,
                    iskworks_sde::PlanetReference {
                        planet_id: row.planet_id,
                        name: row.name_en,
                        solar_system_id: row.solar_system_id,
                        solar_system_name: row.solar_system_name,
                        security_status: row.security_status,
                    },
                )
            })
            .collect())
    }

    async fn reaction_rig_modifiers(
        &self,
        type_id: i64,
        security_class: &str,
        structure_type_id: Option<i64>,
    ) -> Result<Option<iskworks_sde::ReactionRigModifiers>, SdeError> {
        let multiplier_expression = match security_class {
            "highSec" => "rig.high_sec_multiplier",
            "lowSec" => "rig.low_sec_multiplier",
            "nullSec" | "wormhole" => "rig.null_sec_multiplier",
            _ => "1",
        };
        let query = rig_modifier_query("sde_reaction_rig_modifiers", multiplier_expression);
        let row = sqlx::query_as::<_, RigModifierRow>(&query)
            .bind(type_id)
            .bind(structure_type_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sde_sqlx_error)?;
        Ok(row.map(|row| iskworks_sde::ReactionRigModifiers {
            type_id: row.type_id,
            material_reduction_percent: row.material_reduction_percent.normalize().to_string(),
            time_reduction_percent: row.time_reduction_percent.normalize().to_string(),
            compatible_with_structure: row.compatible_with_structure,
            applies_to: row.applicability(),
        }))
    }

    async fn industry_target_filters(
        &self,
    ) -> Result<Vec<iskworks_sde::IndustryTargetFilter>, SdeError> {
        let rows = sqlx::query_as::<_, IndustryTargetFilterRow>(
            r#"SELECT filter.filter_id, filter.name_en, filter.category_ids, filter.group_ids
               FROM sde_imports import
               JOIN sde_industry_target_filters filter ON filter.import_id=import.id
               WHERE import.active=true
               ORDER BY filter.filter_id"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|row| iskworks_sde::IndustryTargetFilter {
                filter_id: row.filter_id,
                name: row.name_en,
                category_ids: row.category_ids,
                group_ids: row.group_ids,
            })
            .collect())
    }

    async fn search_solar_systems(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<iskworks_sde::SolarSystemSearchResult>, SdeError> {
        let query = query.trim();
        if query.len() < 2 {
            return Ok(Vec::new());
        }
        let prefix = format!("{query}%");
        let contains = format!("%{query}%");
        let sql = format!(
            r#"SELECT system.solar_system_id, system.name_en AS solar_system_name,
                      {SECURITY_CLASS_CASE_SQL} AS security_class
               FROM sde_imports import
               JOIN sde_solar_systems system ON system.import_id=import.id
               WHERE import.active=true AND lower(system.name_en) LIKE lower($1)
               ORDER BY CASE
                 WHEN lower(system.name_en)=lower($2) THEN 0
                 WHEN lower(system.name_en) LIKE lower($3) THEN 1
                 ELSE 2 END,
                 system.name_en, system.solar_system_id
               LIMIT $4"#
        );
        sqlx::query_as::<_, SolarSystemSearchRow>(&sql)
            .bind(contains)
            .bind(query)
            .bind(prefix)
            .bind(i64::from(limit.clamp(1, 50)))
            .fetch_all(&self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| iskworks_sde::SolarSystemSearchResult {
                        solar_system_id: row.solar_system_id,
                        solar_system_name: row.solar_system_name,
                        security_class: row.security_class,
                    })
                    .collect()
            })
            .map_err(map_sde_sqlx_error)
    }

    async fn search_npc_stations(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<iskworks_sde::NpcStationSearchResult>, SdeError> {
        let query = query.trim();
        let contains = format!("%{query}%");
        let prefix = format!("{query}%");
        let sql = format!(
            r#"SELECT station.station_id, station.name_en AS station_name,
                      station.station_type_id, station_type.name_en AS station_type_name,
                      system.solar_system_id, system.name_en AS solar_system_name,
                      system.region_id, region.name_en AS region_name,
                      {SECURITY_CLASS_CASE_SQL} AS security_class
               FROM sde_imports import
               JOIN sde_npc_stations station ON station.import_id=import.id
               JOIN sde_solar_systems system
                 ON system.import_id=import.id
                AND system.solar_system_id=station.solar_system_id
               JOIN sde_regions region
                 ON region.import_id=import.id
                AND region.region_id=system.region_id
               LEFT JOIN sde_types station_type
                 ON station_type.import_id=import.id
                AND station_type.type_id=station.station_type_id
               WHERE import.active=true
                 AND (
                   $1=''
                   OR lower(station.name_en) LIKE lower($2)
                   OR lower(system.name_en) LIKE lower($2)
                 )
               ORDER BY CASE
                 WHEN lower(station.name_en)=lower($1) THEN 0
                 WHEN lower(station.name_en) LIKE lower($3) THEN 1
                 ELSE 2 END,
                 station.name_en,station.station_id
               LIMIT $4"#
        );
        sqlx::query_as::<_, NpcStationSearchRow>(&sql)
            .bind(query)
            .bind(contains)
            .bind(prefix)
            .bind(i64::from(limit.clamp(1, 100)))
            .fetch_all(&self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(NpcStationSearchRow::into_result)
                    .collect()
            })
            .map_err(map_sde_sqlx_error)
    }

    async fn resolve_npc_stations(
        &self,
        station_ids: &[i64],
    ) -> Result<Vec<iskworks_sde::NpcStationSearchResult>, SdeError> {
        if station_ids.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            r#"SELECT station.station_id, station.name_en AS station_name,
                      station.station_type_id, station_type.name_en AS station_type_name,
                      system.solar_system_id, system.name_en AS solar_system_name,
                      system.region_id, region.name_en AS region_name,
                      {SECURITY_CLASS_CASE_SQL} AS security_class
               FROM sde_imports import
               JOIN sde_npc_stations station ON station.import_id=import.id
               JOIN sde_solar_systems system
                 ON system.import_id=import.id
                AND system.solar_system_id=station.solar_system_id
               JOIN sde_regions region
                 ON region.import_id=import.id
                AND region.region_id=system.region_id
               LEFT JOIN sde_types station_type
                 ON station_type.import_id=import.id
                AND station_type.type_id=station.station_type_id
               WHERE import.active=true AND station.station_id = ANY($1)
               ORDER BY station.name_en,station.station_id"#
        );
        sqlx::query_as::<_, NpcStationSearchRow>(&sql)
            .bind(station_ids)
            .fetch_all(&self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(NpcStationSearchRow::into_result)
                    .collect()
            })
            .map_err(map_sde_sqlx_error)
    }

    async fn list_regions(&self) -> Result<Vec<iskworks_sde::RegionSummary>, SdeError> {
        // Market-relevant region_id range, verified live against this
        // app's imported SDE: the active dataset has exactly
        // 114 regions. `10000001..=10000069` is precisely the 67 of them
        // that are ordinary, currently-populated k-space (Derelik through
        // Black Rise) -- confirmed by checking every region's own solar
        // systems show natural, varied security_status (spread >= 0.05
        // between their min and max).
        //
        // A simpler "has at least one NPC station" heuristic was tried
        // first and rejected: it wrongly *admits* Thera's wormhole region
        // (11000031, which has 4 NPC stations despite being Anoikis space),
        // and wrongly *excludes* real, actively-used deep-nullsec regions
        // that have zero NPC stations at all (e.g. Feythabolis, Cobalt
        // Edge, Branch, Paragon Soul, Tenerifis, Omist, Period Basis,
        // Perrigen Falls -- sovereignty-null regions with only Upwell
        // structures, the kind of market this selector specifically needs
        // to support browsing to).
        //
        // Everything above 10000069 is deliberately excluded:
        // - 10000070 Pochven: the one region in the classic map whose every
        //   solar system shares the *exact same* security_status (-1.0,
        //   spread 0.0) -- unlike every other region checked, none of which
        //   show this pattern. That uniformity is a genuine data anomaly,
        //   consistent with Pochven's real-world lack of a conventional
        //   player market since the Triglavian invasion changes.
        // - 10001000 Yasna Zakh, 10001004 Exordium, 19000001 GPMR-01: three
        //   one-off regions outside the classic 10000001-10000070 block
        //   entirely, none of them part of the normal 67-region k-space map
        //   most players navigate.
        // - 11000001..=11000033: the 33 Anoikis/wormhole regions (verified
        //   min/max/count over that exact range), already excluded from
        //   "wormhole" classification's own region-membership rule --
        //   see `SECURITY_CLASS_CASE_SQL`.
        // - 12000001..=12000005, 14000001..=14000005: Abyssal Deadspace and
        //   Void regions, confirmed to have zero NPC stations and no
        //   stargate-reachable market presence.
        let sql = format!(
            r#"SELECT r.region_id, r.name_en AS region_name
               FROM sde_imports import
               JOIN sde_regions r ON r.import_id=import.id
               WHERE import.active=true
                 AND {MARKET_RELEVANT_REGION_FILTER_SQL}
               ORDER BY r.name_en"#
        );
        sqlx::query_as::<_, RegionRow>(&sql)
            .fetch_all(&self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| iskworks_sde::RegionSummary {
                        region_id: row.region_id,
                        region_name: row.region_name,
                    })
                    .collect()
            })
            .map_err(map_sde_sqlx_error)
    }

    async fn search_regions(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<iskworks_sde::RegionSummary>, SdeError> {
        let query = query.trim();
        if query.len() < 2 {
            return Ok(Vec::new());
        }
        let contains = format!("%{query}%");
        let prefix = format!("{query}%");
        let sql = format!(
            r#"SELECT r.region_id, r.name_en AS region_name
               FROM sde_imports import
               JOIN sde_regions r ON r.import_id=import.id
               WHERE import.active=true
                 AND {MARKET_RELEVANT_REGION_FILTER_SQL}
                 AND lower(r.name_en) LIKE lower($1)
               ORDER BY CASE
                 WHEN lower(r.name_en)=lower($2) THEN 0
                 WHEN lower(r.name_en) LIKE lower($3) THEN 1
                 ELSE 2 END,
                 r.name_en
               LIMIT $4"#
        );
        sqlx::query_as::<_, RegionRow>(&sql)
            .bind(contains)
            .bind(query)
            .bind(prefix)
            .bind(i64::from(limit.clamp(1, 50)))
            .fetch_all(&self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| iskworks_sde::RegionSummary {
                        region_id: row.region_id,
                        region_name: row.region_name,
                    })
                    .collect()
            })
            .map_err(map_sde_sqlx_error)
    }

    async fn list_npc_stations_in_region(
        &self,
        region_id: i64,
    ) -> Result<Vec<iskworks_sde::NpcStationSearchResult>, SdeError> {
        let sql = format!(
            r#"SELECT station.station_id, station.name_en AS station_name,
                      station.station_type_id, station_type.name_en AS station_type_name,
                      system.solar_system_id, system.name_en AS solar_system_name,
                      system.region_id, region.name_en AS region_name,
                      {SECURITY_CLASS_CASE_SQL} AS security_class
               FROM sde_imports import
               JOIN sde_npc_stations station ON station.import_id=import.id
               JOIN sde_solar_systems system
                 ON system.import_id=import.id
                AND system.solar_system_id=station.solar_system_id
               JOIN sde_regions region
                 ON region.import_id=import.id
                AND region.region_id=system.region_id
               LEFT JOIN sde_types station_type
                 ON station_type.import_id=import.id
                AND station_type.type_id=station.station_type_id
               WHERE import.active=true AND system.region_id=$1
               ORDER BY station.name_en,station.station_id"#
        );
        sqlx::query_as::<_, NpcStationSearchRow>(&sql)
            .bind(region_id)
            .fetch_all(&self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(NpcStationSearchRow::into_result)
                    .collect()
            })
            .map_err(map_sde_sqlx_error)
    }

    async fn list_market_groups(&self) -> Result<Vec<iskworks_sde::MarketGroupNode>, SdeError> {
        sqlx::query_as::<_, MarketGroupRow>(
            r#"
            SELECT mg.market_group_id, mg.name_en AS name, mg.parent_group_id,
                   COALESCE(counts.item_count, 0) AS item_count
            FROM sde_imports import
            JOIN sde_market_groups mg ON mg.import_id=import.id
            LEFT JOIN (
              SELECT t.market_group_id, count(*) AS item_count
              FROM sde_imports import
              JOIN sde_types t ON t.import_id=import.id
              WHERE import.active=true AND t.published=true AND t.market_group_id IS NOT NULL
              GROUP BY t.market_group_id
            ) counts ON counts.market_group_id=mg.market_group_id
            WHERE import.active=true
            ORDER BY mg.market_group_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| iskworks_sde::MarketGroupNode {
                    market_group_id: row.market_group_id,
                    name: row.name,
                    parent_group_id: row.parent_group_id,
                    item_count: u64::try_from(row.item_count).unwrap_or(0),
                })
                .collect()
        })
        .map_err(map_sde_sqlx_error)
    }

    async fn list_market_items(
        &self,
        market_group_id: Option<i64>,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<iskworks_sde::MarketItemCandidate>, u64), SdeError> {
        let search = search.trim();
        let contains = format!("%{search}%");
        let offset = i64::from(page.saturating_sub(1)) * i64::from(page_size);

        let total_count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM sde_imports import
            JOIN sde_types t ON t.import_id=import.id
            WHERE import.active=true AND t.published=true AND t.market_group_id IS NOT NULL
              AND ($1::bigint IS NULL OR t.market_group_id=$1)
              AND ($2='' OR lower(t.name_en) LIKE lower($3))
            "#,
        )
        .bind(market_group_id)
        .bind(search)
        .bind(&contains)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        let rows = sqlx::query_as::<_, MarketItemCandidateRow>(
            r#"
            SELECT t.type_id, t.name_en AS type_name
            FROM sde_imports import
            JOIN sde_types t ON t.import_id=import.id
            WHERE import.active=true AND t.published=true AND t.market_group_id IS NOT NULL
              AND ($1::bigint IS NULL OR t.market_group_id=$1)
              AND ($2='' OR lower(t.name_en) LIKE lower($3))
            ORDER BY t.name_en, t.type_id
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(market_group_id)
        .bind(search)
        .bind(&contains)
        .bind(i64::from(page_size))
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        Ok((
            rows.into_iter()
                .map(|row| iskworks_sde::MarketItemCandidate {
                    type_id: row.type_id,
                    type_name: row.type_name,
                })
                .collect(),
            u64::try_from(total_count).unwrap_or(0),
        ))
    }

    async fn list_market_group_subtree_item_ids(
        &self,
        root_group_id: i64,
    ) -> Result<Vec<iskworks_sde::MarketItemCandidate>, SdeError> {
        // Same recursive-descendant-walk shape as `manufacturable_candidates`'s
        // `market_tree` CTE above, specialized to a single root rather than
        // `ANY($n)` -- `parent_group_id` is single-valued per group, so this
        // tree walk structurally cannot revisit a group twice, meaning each
        // type_id is emitted exactly once regardless of nesting depth.
        let rows = sqlx::query_as::<_, MarketItemCandidateRow>(
            r#"
            WITH RECURSIVE market_tree AS (
              SELECT import_id, market_group_id FROM sde_market_groups
              WHERE market_group_id = $1
              UNION ALL
              SELECT child.import_id, child.market_group_id
              FROM sde_market_groups child JOIN market_tree parent
                ON child.import_id = parent.import_id AND child.parent_group_id = parent.market_group_id
            )
            SELECT t.type_id, t.name_en AS type_name
            FROM sde_imports import
            JOIN sde_types t ON t.import_id=import.id
            JOIN market_tree tree ON tree.import_id=t.import_id AND tree.market_group_id=t.market_group_id
            WHERE import.active=true AND t.published=true
            ORDER BY t.name_en, t.type_id
            "#,
        )
        .bind(root_group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sde_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|row| iskworks_sde::MarketItemCandidate {
                type_id: row.type_id,
                type_name: row.type_name,
            })
            .collect())
    }
}

#[derive(sqlx::FromRow)]
struct MarketItemCandidateRow {
    type_id: i64,
    type_name: String,
}

#[derive(sqlx::FromRow)]
struct RegionRow {
    region_id: i64,
    region_name: String,
}

#[derive(sqlx::FromRow)]
struct MarketGroupRow {
    market_group_id: i64,
    name: String,
    parent_group_id: Option<i64>,
    item_count: i64,
}

#[derive(sqlx::FromRow)]
struct SolarSystemSearchRow {
    solar_system_id: i64,
    solar_system_name: String,
    security_class: String,
}

#[derive(sqlx::FromRow)]
struct NpcStationSearchRow {
    station_id: i64,
    station_name: String,
    station_type_id: i64,
    station_type_name: Option<String>,
    solar_system_id: i64,
    solar_system_name: String,
    region_id: i64,
    region_name: String,
    security_class: String,
}

impl NpcStationSearchRow {
    fn into_result(self) -> iskworks_sde::NpcStationSearchResult {
        iskworks_sde::NpcStationSearchResult {
            station_id: self.station_id,
            station_name: self.station_name,
            station_type_id: self.station_type_id,
            station_type_name: self.station_type_name,
            solar_system_id: self.solar_system_id,
            solar_system_name: self.solar_system_name,
            region_id: self.region_id,
            region_name: self.region_name,
            security_class: self.security_class,
        }
    }
}

#[derive(sqlx::FromRow)]
struct CandidateHeaderRow {
    kind: String,
    import_id: Uuid,
    source_version: String,
    recipe_id: i64,
    recipe_name: String,
    duration_seconds: Option<i64>,
    product_type_id: i64,
    product_count: i64,
    product_published: bool,
    recipe_published: bool,
    group_id: Option<i64>,
    group_name: Option<String>,
    category_id: Option<i64>,
    category_name: Option<String>,
    meta_group_id: Option<i64>,
    meta_group_name: Option<String>,
    market_group_id: Option<i64>,
    market_group_name: Option<String>,
    market_group_ancestor_ids: Vec<i64>,
    market_group_ancestor_names: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct CandidateLineRow {
    kind: String,
    recipe_id: i64,
    type_id: i64,
    type_name: String,
    quantity: i64,
}

async fn candidate_lines(
    pool: &PgPool,
    manufacturing_ids: &[i64],
    reaction_ids: &[i64],
    materials: bool,
) -> Result<BTreeMap<(String, i64), Vec<RecipeLine>>, SdeError> {
    let sql = if materials {
        r#"
        SELECT 'manufacturing'::text kind, line.blueprint_type_id recipe_id,
               line.material_type_id type_id, item.name_en type_name, line.quantity
        FROM sde_imports i JOIN sde_blueprint_materials line ON line.import_id=i.id
        JOIN sde_types item ON item.import_id=line.import_id AND item.type_id=line.material_type_id
        WHERE i.active AND line.blueprint_type_id = ANY($1)
        UNION ALL
        SELECT 'reaction', line.reaction_formula_type_id, line.material_type_id, item.name_en, line.quantity
        FROM sde_imports i JOIN sde_reaction_formula_materials line ON line.import_id=i.id
        JOIN sde_types item ON item.import_id=line.import_id AND item.type_id=line.material_type_id
        WHERE i.active AND line.reaction_formula_type_id = ANY($2)
        ORDER BY 1, 2, 3
        "#
    } else {
        r#"
        SELECT 'manufacturing'::text kind, line.blueprint_type_id recipe_id,
               line.product_type_id type_id, item.name_en type_name, line.quantity
        FROM sde_imports i JOIN sde_blueprint_products line ON line.import_id=i.id
        JOIN sde_types item ON item.import_id=line.import_id AND item.type_id=line.product_type_id
        WHERE i.active AND line.blueprint_type_id = ANY($1)
        UNION ALL
        SELECT 'reaction', line.reaction_formula_type_id, line.product_type_id, item.name_en, line.quantity
        FROM sde_imports i JOIN sde_reaction_formula_products line ON line.import_id=i.id
        JOIN sde_types item ON item.import_id=line.import_id AND item.type_id=line.product_type_id
        WHERE i.active AND line.reaction_formula_type_id = ANY($2)
        ORDER BY 1, 2, 3
        "#
    };
    let rows = sqlx::query_as::<_, CandidateLineRow>(sql)
        .bind(manufacturing_ids)
        .bind(reaction_ids)
        .fetch_all(pool)
        .await
        .map_err(map_sde_sqlx_error)?;
    let mut grouped = BTreeMap::new();
    for row in rows {
        grouped
            .entry((row.kind, row.recipe_id))
            .or_insert_with(Vec::new)
            .push(RecipeLine {
                type_id: row.type_id,
                type_name: row.type_name,
                quantity: row.quantity,
            });
    }
    Ok(grouped)
}

#[derive(sqlx::FromRow)]
struct StructureModifierRow {
    type_id: i64,
    material_reduction_percent: rust_decimal::Decimal,
    time_reduction_percent: rust_decimal::Decimal,
    job_cost_reduction_percent: rust_decimal::Decimal,
}

/// Shared SELECT for `rig_manufacturing_modifiers` / `reaction_rig_modifiers`
/// -- identical apart from the rig table. `$1` = rig type id, `$2` = the
/// structure type id to test fitting compatibility against. `table` and
/// `multiplier` are fixed internal strings, never user input.
///
/// A rig activity dimension (`material_filter_ids` / `time_filter_ids`) may
/// reference several target filters (a multi-scope rig); the resolved
/// applicability is the *union* of those filters' category/group sets.
/// `*_filter_count` is how many ids the rig declares -- restricted iff > 0,
/// even when none resolve, so an unresolvable id can never widen a rig back
/// to "applies everywhere".
fn rig_modifier_query(table: &str, multiplier: &str) -> String {
    let dimension = |alias: &str, array_col: &str| {
        format!(
            "LEFT JOIN LATERAL (
               SELECT
                 COALESCE(CARDINALITY(rig.{array_col}), 0)::bigint AS filter_count,
                 (SELECT MIN(f.filter_id) FROM sde_industry_target_filters f
                    WHERE f.import_id = import.id AND f.filter_id = ANY(rig.{array_col})) AS filter_min_id,
                 (SELECT STRING_AGG(f.name_en, ', ' ORDER BY f.filter_id)
                    FROM sde_industry_target_filters f
                    WHERE f.import_id = import.id AND f.filter_id = ANY(rig.{array_col})) AS filter_name,
                 COALESCE((SELECT ARRAY_AGG(DISTINCT v ORDER BY v)
                    FROM sde_industry_target_filters f
                    CROSS JOIN LATERAL UNNEST(f.category_ids) AS v
                    WHERE f.import_id = import.id AND f.filter_id = ANY(rig.{array_col})), '{{}}'::bigint[]) AS category_ids,
                 COALESCE((SELECT ARRAY_AGG(DISTINCT v ORDER BY v)
                    FROM sde_industry_target_filters f
                    CROSS JOIN LATERAL UNNEST(f.group_ids) AS v
                    WHERE f.import_id = import.id AND f.filter_id = ANY(rig.{array_col})), '{{}}'::bigint[]) AS group_ids
             ) {alias} ON true"
        )
    };
    format!(
        r#"SELECT rig.type_id,
                  rig.material_reduction_percent * {multiplier} AS material_reduction_percent,
                  rig.time_reduction_percent * {multiplier} AS time_reduction_percent,
                  CASE WHEN structure_type.type_id IS NULL THEN NULL
                       ELSE structure_type.group_id = ANY(rig.compatible_structure_group_ids)
                         AND structure.structure_size = rig.rig_size
                  END AS compatible_with_structure,
                  mat.filter_count AS material_filter_count,
                  mat.filter_min_id AS material_filter_min_id,
                  mat.filter_name AS material_filter_name,
                  mat.category_ids AS material_filter_category_ids,
                  mat.group_ids AS material_filter_group_ids,
                  tim.filter_count AS time_filter_count,
                  tim.filter_min_id AS time_filter_min_id,
                  tim.filter_name AS time_filter_name,
                  tim.category_ids AS time_filter_category_ids,
                  tim.group_ids AS time_filter_group_ids
           FROM sde_imports import
           JOIN {table} rig ON rig.import_id=import.id
           LEFT JOIN sde_types structure_type
             ON structure_type.import_id=import.id AND structure_type.type_id=$2
           LEFT JOIN sde_structure_manufacturing_modifiers structure
             ON structure.import_id=import.id AND structure.type_id=structure_type.type_id
           {material_dimension}
           {time_dimension}
           WHERE import.active=true AND rig.type_id=$1"#,
        material_dimension = dimension("mat", "material_filter_ids"),
        time_dimension = dimension("tim", "time_filter_ids"),
    )
}

#[derive(sqlx::FromRow)]
struct IndustryTargetFilterRow {
    filter_id: i64,
    name_en: String,
    category_ids: Vec<i64>,
    group_ids: Vec<i64>,
}

#[derive(sqlx::FromRow)]
struct RigModifierRow {
    type_id: i64,
    material_reduction_percent: rust_decimal::Decimal,
    time_reduction_percent: rust_decimal::Decimal,
    compatible_with_structure: Option<bool>,
    material_filter_count: i64,
    material_filter_min_id: Option<i64>,
    material_filter_name: Option<String>,
    material_filter_category_ids: Vec<i64>,
    material_filter_group_ids: Vec<i64>,
    time_filter_count: i64,
    time_filter_min_id: Option<i64>,
    time_filter_name: Option<String>,
    time_filter_category_ids: Vec<i64>,
    time_filter_group_ids: Vec<i64>,
}

impl RigModifierRow {
    fn applicability(&self) -> iskworks_sde::RigApplicability {
        // A dimension is restricted iff the rig declares at least one target
        // filter; the category/group sets are the union across every
        // referenced filter. `filter_id` on the synthetic result is the
        // lowest referenced id -- a stable representative that stays equal
        // between material and time when they target the same filter set.
        let filter = |count: i64,
                      min_id: Option<i64>,
                      name: &Option<String>,
                      category_ids: &[i64],
                      group_ids: &[i64]| {
            (count > 0).then(|| iskworks_sde::IndustryTargetFilter {
                filter_id: min_id.unwrap_or_default(),
                name: name.clone().unwrap_or_default(),
                category_ids: category_ids.to_vec(),
                group_ids: group_ids.to_vec(),
            })
        };
        iskworks_sde::RigApplicability {
            material: filter(
                self.material_filter_count,
                self.material_filter_min_id,
                &self.material_filter_name,
                &self.material_filter_category_ids,
                &self.material_filter_group_ids,
            ),
            time: filter(
                self.time_filter_count,
                self.time_filter_min_id,
                &self.time_filter_name,
                &self.time_filter_category_ids,
                &self.time_filter_group_ids,
            ),
        }
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct ImportIdentityRow {
    pub(super) id: Uuid,
    pub(super) source_version: String,
    pub(super) type_count: i64,
    pub(super) blueprint_count: i64,
    pub(super) material_line_count: i64,
    pub(super) product_line_count: i64,
    pub(super) skipped_blueprint_count: i64,
    pub(super) reaction_formula_count: i64,
    pub(super) reaction_material_line_count: i64,
    pub(super) reaction_product_line_count: i64,
    pub(super) skipped_reaction_formula_count: i64,
    pub(super) category_count: i64,
    pub(super) group_count: i64,
    pub(super) meta_group_count: i64,
    pub(super) market_group_count: i64,
    pub(super) classified_type_count: i64,
}

#[derive(sqlx::FromRow)]
struct ActiveImportRow {
    id: Uuid,
    source_version: String,
    source_label: String,
    source_checksum: String,
    completed_at: DateTime<Utc>,
    type_count: i64,
    blueprint_count: i64,
    material_line_count: i64,
    product_line_count: i64,
    skipped_blueprint_count: i64,
    reaction_formula_count: i64,
    reaction_material_line_count: i64,
    reaction_product_line_count: i64,
    skipped_reaction_formula_count: i64,
    category_count: i64,
    group_count: i64,
    meta_group_count: i64,
    market_group_count: i64,
    classified_type_count: i64,
}

#[derive(sqlx::FromRow)]
struct BlueprintSearchRow {
    blueprint_type_id: i64,
    blueprint_name: String,
    product_type_id: i64,
    product_name: String,
    group_name: Option<String>,
    published: bool,
}

#[derive(sqlx::FromRow)]
struct ReactionFormulaSearchRow {
    reaction_formula_type_id: i64,
    reaction_formula_name: String,
    product_type_id: i64,
    product_name: String,
    group_name: Option<String>,
    published: bool,
}

#[derive(sqlx::FromRow)]
struct ReactionFormulaHeaderRow {
    reaction_formula_type_id: i64,
    name_en: String,
    duration_seconds: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct RecipeHeaderRow {
    blueprint_type_id: i64,
    name_en: String,
    duration_seconds: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct RecipeLineRow {
    type_id: i64,
    type_name: String,
    quantity: i64,
}

#[derive(sqlx::FromRow)]
struct TypeSearchRow {
    type_id: i64,
    type_name: String,
    group_name: Option<String>,
    published: bool,
}

#[derive(sqlx::FromRow)]
struct TypeGroupNameRow {
    type_id: i64,
    group_name_en: Option<String>,
}

#[derive(sqlx::FromRow)]
struct TypeNameRow {
    type_id: i64,
    name_en: String,
}

#[derive(sqlx::FromRow)]
struct PlanetSchematicLineRow {
    schematic_id: i64,
    name_en: String,
    cycle_time_seconds: i64,
    type_id: i64,
    is_input: bool,
    quantity: i64,
}

#[derive(sqlx::FromRow)]
struct PlanetReferenceRow {
    planet_id: i64,
    name_en: String,
    solar_system_id: i64,
    solar_system_name: String,
    security_status: Option<rust_decimal::Decimal>,
}

#[derive(sqlx::FromRow)]
struct InventoryTypeMetadataRow {
    type_id: i64,
    group_name_en: Option<String>,
    packaged_volume_m3: Option<rust_decimal::Decimal>,
}

#[derive(sqlx::FromRow)]
struct TypeClassificationRow {
    type_id: i64,
    group_id: Option<i64>,
    group_name: Option<String>,
    category_id: Option<i64>,
    category_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct TypeReferenceRow {
    type_id: i64,
    type_name: String,
    group_id: Option<i64>,
    group_name: Option<String>,
    category_id: Option<i64>,
    category_name: Option<String>,
    packaged_volume_m3: Option<rust_decimal::Decimal>,
}

impl TypeSearchRow {
    fn into_result(self) -> TypeSearchResult {
        TypeSearchResult {
            type_id: self.type_id,
            type_name: self.type_name,
            group_name: self.group_name,
            published: self.published,
        }
    }
}

impl RecipeLineRow {
    fn into_line(self) -> RecipeLine {
        RecipeLine {
            type_id: self.type_id,
            type_name: self.type_name,
            quantity: self.quantity,
        }
    }
}

pub(super) fn map_sde_sqlx_error(error: sqlx::Error) -> SdeError {
    SdeError::Storage(error.to_string())
}
