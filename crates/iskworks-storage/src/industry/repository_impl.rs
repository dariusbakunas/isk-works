use super::*;

#[async_trait]
impl IndustryRepository for PgIndustryRepository {
    async fn list_builds(&self, workspace_id: WorkspaceId) -> Result<Vec<Build>, IndustryError> {
        let ids = sqlx::query_scalar::<_, Uuid>(
            // Only plan roots: a producer belongs to its root's plan.
            "SELECT id FROM builds WHERE workspace_id = $1 AND plan_root_build_id = id \
             ORDER BY updated_at DESC",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        let mut builds = Vec::with_capacity(ids.len());
        for id in ids {
            builds.push(self.load_build(workspace_id, BuildId(id)).await?);
        }
        Ok(builds)
    }

    async fn get_build(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<Build, IndustryError> {
        self.load_build(workspace_id, build_id).await
    }

    async fn load_root_plan(
        &self,
        workspace_id: WorkspaceId,
        root_build_id: BuildId,
    ) -> Result<iskworks_core::production_dependency::RootPlanRecords, IndustryError> {
        // Bounded: a fixed number of statements regardless of the plan's
        // Build count (builds, recipe materials, recipe products, draft
        // planning, edges, active SDE, classifications, owned blueprints,
        // legacy observations), plus SDE recipe reads only for *distinct
        // stale* recipes (none while the captured SDE is the active one).
        let rows: Vec<BuildRow> = sqlx::query_as(
            r#"
            SELECT id, workspace_id, owner_id, display_name, recipe_kind,
                   blueprint_type_id, blueprint_name, reaction_formula_type_id,
                   reaction_formula_name, duration_seconds_per_run, source_sde_dataset_id,
                   source_sde_version, recipe_fingerprint, runs, notes,
                   revision, created_at, updated_at
            FROM builds
            WHERE workspace_id = $1 AND (id = $2 OR plan_root_build_id = $2)
            ORDER BY id
            "#,
        )
        .bind(workspace_id.0)
        .bind(root_build_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        if !rows.iter().any(|row| row.id == root_build_id.0) {
            return Err(IndustryError::BuildNotFound);
        }
        let owns_its_plan: bool = sqlx::query_scalar(
            "SELECT plan_root_build_id IS NOT DISTINCT FROM id FROM builds WHERE id = $1",
        )
        .bind(root_build_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        if !owns_its_plan {
            return Err(IndustryError::Validation(
                "a root plan is loaded from its root Build".to_string(),
            ));
        }
        let ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();

        let mut materials: std::collections::HashMap<Uuid, Vec<CapturedRecipeLine>> =
            std::collections::HashMap::new();
        for row in sqlx::query_as::<_, PlanRecipeLineRow>(
            "SELECT build_id, type_id, captured_name, quantity_per_run, sort_order \
             FROM build_recipe_materials WHERE build_id = ANY($1) ORDER BY build_id, sort_order",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        {
            let (build_id, line) = row.into_line()?;
            materials.entry(build_id).or_default().push(line);
        }
        let mut products: std::collections::HashMap<Uuid, Vec<CapturedRecipeLine>> =
            std::collections::HashMap::new();
        for row in sqlx::query_as::<_, PlanRecipeLineRow>(
            "SELECT build_id, type_id, captured_name, quantity_per_run, sort_order \
             FROM build_recipe_products WHERE build_id = ANY($1) ORDER BY build_id, sort_order",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        {
            let (build_id, line) = row.into_line()?;
            products.entry(build_id).or_default().push(line);
        }
        let mut drafts: std::collections::HashMap<Uuid, DraftPlanningSnapshot> =
            std::collections::HashMap::new();
        for (build_id, planning_input, updated_at) in
            sqlx::query_as::<_, (Uuid, serde_json::Value, DateTime<Utc>)>(
                "SELECT build_id, planning_input, updated_at FROM build_draft_planning \
                 WHERE build_id = ANY($1)",
            )
            .bind(&ids)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)?
        {
            let input =
                serde_json::from_value::<DraftPlanningInput>(planning_input).map_err(|error| {
                    IndustryError::Persistence(format!(
                        "invalid stored draft planning input: {error}"
                    ))
                })?;
            drafts.insert(build_id, DraftPlanningSnapshot { input, updated_at });
        }
        let dependencies = sqlx::query_as::<_, ProductionDependencyRow>(
            r#"
            SELECT id, plan_root_build_id, consumer_build_id, component_type_id, sourcing,
                   method_kind, method_type_id, producer_build_id, fulfillment_scope,
                   revision, created_at, updated_at
            FROM production_dependencies
            WHERE workspace_id = $1 AND plan_root_build_id = $2
            ORDER BY consumer_build_id, component_type_id
            "#,
        )
        .bind(workspace_id.0)
        .bind(root_build_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(ProductionDependencyRow::into_domain)
        .collect::<Result<Vec<_>, _>>()?;

        // Read-model fields, bulk (same values `load_build` resolves one
        // Build at a time).
        let sde = PgSdeRepository::new(self.pool.clone());
        let active = sde.active_sde().await.ok().flatten();
        let mut currency_cache: std::collections::HashMap<
            (String, Option<i64>, Option<i64>, Uuid, String),
            (RecipeCurrency, Option<String>),
        > = std::collections::HashMap::new();
        let mut builds = Vec::with_capacity(rows.len());
        for row in rows {
            let build_materials = materials.remove(&row.id).unwrap_or_default();
            let build_products = products.remove(&row.id).unwrap_or_default();
            let key = (
                row.recipe_kind.clone(),
                row.blueprint_type_id,
                row.reaction_formula_type_id,
                row.source_sde_dataset_id,
                row.recipe_fingerprint.clone(),
            );
            let (recipe_currency, active_sde_version) = match currency_cache.get(&key) {
                Some(cached) => cached.clone(),
                None => {
                    let computed = self
                        .compare_recipe_to(active.as_ref(), &row, &build_materials, &build_products)
                        .await;
                    currency_cache.insert(key, computed.clone());
                    computed
                }
            };
            let draft = drafts.remove(&row.id);
            builds.push(row.into_build(
                build_materials,
                build_products,
                draft,
                recipe_currency,
                active_sde_version,
            )?);
        }

        let product_type_ids: Vec<i64> = builds
            .iter()
            .map(|build| build.recipe.primary_product().type_id)
            .collect();
        if let Ok(classifications) = sde.type_classifications(&product_type_ids).await {
            for build in &mut builds {
                if let Some(classification) =
                    classifications.get(&build.recipe.primary_product().type_id)
                {
                    build
                        .product_category_name
                        .clone_from(&classification.category_name);
                    build
                        .product_group_name
                        .clone_from(&classification.group_name);
                }
            }
        }
        let blueprint_type_ids: Vec<i64> = builds
            .iter()
            .filter_map(|build| build.recipe.blueprint_type_id())
            .collect();
        let owned: std::collections::HashSet<i64> = sqlx::query_scalar::<_, i64>(
            "SELECT DISTINCT blueprint_type_id FROM blueprint_observations \
             WHERE workspace_id = $1 AND blueprint_type_id = ANY($2)",
        )
        .bind(workspace_id.0)
        .bind(&blueprint_type_ids)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();
        let legacy_observations: Vec<Uuid> = builds
            .iter()
            .filter_map(|build| {
                match build
                    .draft_planning
                    .as_ref()?
                    .input
                    .blueprint_selection
                    .as_ref()?
                {
                    BlueprintSelection::ObservedAsset {
                        observation_id,
                        kind: BlueprintKind::Unknown,
                        ..
                    } => Some(*observation_id),
                    _ => None,
                }
            })
            .collect();
        let observed_kinds: std::collections::HashMap<Uuid, BlueprintKind> =
            if legacy_observations.is_empty() {
                std::collections::HashMap::new()
            } else {
                sqlx::query_as::<_, (Uuid, String)>(
                    "SELECT id, blueprint_kind FROM blueprint_observations \
                     WHERE workspace_id = $1 AND id = ANY($2)",
                )
                .bind(workspace_id.0)
                .bind(&legacy_observations)
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(id, kind)| parse_blueprint_kind(&kind).ok().map(|kind| (id, kind)))
                .collect()
            };
        for build in &mut builds {
            build.has_owned_blueprint = build
                .recipe
                .blueprint_type_id()
                .is_some_and(|id| owned.contains(&id));
            build.selected_blueprint_origin = build
                .draft_planning
                .as_ref()
                .and_then(|draft| draft.input.blueprint_selection.as_ref())
                .and_then(|selection| match selection {
                    BlueprintSelection::Manual { kind, .. } => Some(*kind),
                    BlueprintSelection::ObservedAsset { kind, .. }
                        if *kind != BlueprintKind::Unknown =>
                    {
                        Some(*kind)
                    }
                    BlueprintSelection::ObservedAsset { observation_id, .. } => {
                        observed_kinds.get(observation_id).copied()
                    }
                });
        }

        let retired_producers: Vec<BuildId> = sqlx::query_scalar::<_, Uuid>(
            "SELECT producer_build_id FROM retired_producers \
             WHERE workspace_id = $1 AND plan_root_build_id = $2 ORDER BY producer_build_id",
        )
        .bind(workspace_id.0)
        .bind(root_build_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(BuildId)
        .collect();

        let root_index = builds
            .iter()
            .position(|build| build.id == root_build_id)
            .expect("root row was found above");
        let root = builds.remove(root_index);
        Ok(iskworks_core::production_dependency::RootPlanRecords {
            root,
            producers: builds,
            dependencies,
            retired_producers,
        })
    }

    async fn plan_root_of(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<Option<BuildId>, IndustryError> {
        let root: Option<Uuid> = sqlx::query_scalar(
            "SELECT plan_root_build_id FROM builds WHERE workspace_id = $1 AND id = $2",
        )
        .bind(workspace_id.0)
        .bind(build_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?;
        root.map(|root| Some(BuildId(root)))
            .ok_or(IndustryError::BuildNotFound)
    }

    async fn apply_canonical_consumer_write(
        &self,
        workspace_id: WorkspaceId,
        write: iskworks_core::canonical_planner::CanonicalConsumerWrite,
    ) -> Result<Build, IndustryError> {
        let consumer = write.consumer;
        apply_canonical_consumer_write(&self.pool, workspace_id, write).await?;
        self.load_build(workspace_id, consumer).await
    }

    async fn create_root_build(&self, new_build: NewBuild) -> Result<Build, IndustryError> {
        let build_id = new_build.build.id;
        let workspace_id = new_build.build.workspace_id;
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        // Inserted as its own plan root.
        insert_build(&mut tx, &new_build.build).await?;
        // One Buy edge per material; canonical writes address edges by id,
        // and the service applies the draft's own sourcing through them.
        insert_buy_edges(&mut tx, build_id, build_id).await?;
        tx.commit().await.map_err(map_error)?;
        self.load_build(workspace_id, build_id).await
    }

    async fn create_build(&self, new_build: NewBuild) -> Result<Build, IndustryError> {
        let build = new_build.build;
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        insert_build(&mut tx, &build).await?;
        tx.commit().await.map_err(map_error)?;
        self.load_build(build.workspace_id, build.id).await
    }

    async fn update_draft(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        update: DraftUpdate,
    ) -> Result<Build, IndustryError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        guard_sourcing_unchanged(&mut tx, build_id, &update).await?;
        // A recipe replacement is a sourcing change: the guard refuses it
        // here, so it only ever goes through the canonical consumer write.
        let result = sqlx::query(
            r#"
            UPDATE builds SET display_name = $1, runs = $2, notes = $3,
              revision = revision + 1, updated_at = $4
            WHERE workspace_id = $5 AND id = $6 AND revision = $7
            "#,
        )
        .bind(&update.name)
        .bind(i64_from_u64(update.runs)?)
        .bind(&update.notes)
        .bind(crate::db_now())
        .bind(workspace_id.0)
        .bind(build_id.0)
        .bind(i64_from_u64(update.expected_revision)?)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return Err(classify_build_update(&mut tx, workspace_id, build_id).await?);
        }
        write_draft_planning(&mut tx, build_id, update.draft_planning.as_ref()).await?;
        tx.commit().await.map_err(map_error)?;
        self.load_build(workspace_id, build_id).await
    }

    async fn update_draft_planning_batch(
        &self,
        workspace_id: WorkspaceId,
        updates: Vec<DraftPlanningBatchUpdate>,
    ) -> Result<Vec<Build>, IndustryError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let now = crate::db_now();
        for update in &updates {
            let expected_revision = i64_from_u64(update.expected_revision)?;
            let result = sqlx::query(
                r#"
                UPDATE builds SET revision = revision + 1, updated_at = $1
                WHERE workspace_id = $2 AND id = $3 AND revision = $4
                "#,
            )
            .bind(now)
            .bind(workspace_id.0)
            .bind(update.build_id.0)
            .bind(expected_revision)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
            if result.rows_affected() == 0 {
                // Neither committed nor rolled back explicitly -- `tx` is
                // simply dropped here, which rolls back every prior member's
                // write in this same batch too. No partial application.
                return Err(classify_build_update(&mut tx, workspace_id, update.build_id).await?);
            }
            write_draft_planning(
                &mut tx,
                update.build_id,
                Some(&DraftPlanningSnapshot {
                    input: update.input.clone(),
                    updated_at: now,
                }),
            )
            .await?;
        }
        tx.commit().await.map_err(map_error)?;

        let mut builds = Vec::with_capacity(updates.len());
        for update in updates {
            builds.push(self.load_build(workspace_id, update.build_id).await?);
        }
        Ok(builds)
    }

    async fn rename_build(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        name: String,
    ) -> Result<Build, IndustryError> {
        let result = sqlx::query(
            r#"
            UPDATE builds
            SET display_name = $1
            WHERE workspace_id = $2 AND id = $3
            "#,
        )
        .bind(name)
        .bind(workspace_id.0)
        .bind(build_id.0)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        if result.rows_affected() == 0 {
            return match self.load_build(workspace_id, build_id).await {
                Ok(_) => Err(IndustryError::RevisionConflict),
                Err(error) => Err(error),
            };
        }
        self.load_build(workspace_id, build_id).await
    }

    async fn delete_build(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
        expected_revision: u64,
        _force: bool,
    ) -> Result<(), IndustryError> {
        let expected_revision = i64_from_u64(expected_revision)?;
        let mut tx = self.pool.begin().await.map_err(map_error)?;

        // The build plus, for a plan root, every Build of its plan: those
        // cascade-delete with it (`plan_root_build_id`), and a ticket /
        // requirement / prerequisite may pin any of them
        // (`source_build_id`), not just the root. A producer's plan root is
        // never itself, so deleting a producer deletes only that Build.
        let subtree: Vec<Uuid> = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM builds WHERE workspace_id = $1 AND (id = $2 OR plan_root_build_id = $2)",
        )
        .bind(workspace_id.0)
        .bind(build_id.0)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?;

        if subtree.is_empty() {
            return Err(IndustryError::BuildNotFound);
        }

        // A producer still referenced by an active demand edge from outside
        // the deleted set cannot go: its consumers would silently lose their
        // production. (Whole-plan deletes never trip this -- every consumer
        // is in the set.) Orphaned producers -- no incoming
        // edge -- follow the ordinary deletion policy below.
        let referenced: Vec<(Uuid, Uuid)> = sqlx::query_as(
            r#"
            SELECT producer_build_id, consumer_build_id
            FROM production_dependencies
            WHERE producer_build_id = ANY($1) AND NOT (consumer_build_id = ANY($1))
            ORDER BY producer_build_id, consumer_build_id
            "#,
        )
        .bind(&subtree)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_error)?;
        if let Some((producer, _)) = referenced.first().copied() {
            return Err(IndustryError::ProducerInUse {
                producer: BuildId(producer),
                consumers: referenced
                    .iter()
                    .filter(|(p, _)| *p == producer)
                    .map(|(_, consumer)| BuildId(*consumer))
                    .collect(),
            });
        }

        // Build-only snapshots have no meaning after their source disappears,
        // but an Epic's frozen economics must survive. Referenced snapshots
        // detach through ON DELETE SET NULL with the rest of the provenance.
        sqlx::query(
            r#"
            DELETE FROM price_snapshots ps
            WHERE ps.build_id = ANY($1)
              AND NOT EXISTS (
                    SELECT 1 FROM orders o WHERE o.price_snapshot_id = ps.id
                  )
            "#,
        )
        .bind(&subtree)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;

        let result =
            sqlx::query("DELETE FROM builds WHERE workspace_id = $1 AND id = $2 AND revision = $3")
                .bind(workspace_id.0)
                .bind(build_id.0)
                .bind(expected_revision)
                .execute(&mut *tx)
                .await
                .map_err(map_error)?;
        if result.rows_affected() == 0 {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM builds WHERE workspace_id = $1 AND id = $2)",
            )
            .bind(workspace_id.0)
            .bind(build_id.0)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_error)?;
            return Err(if exists {
                IndustryError::RevisionConflict
            } else {
                IndustryError::BuildNotFound
            });
        }

        tx.commit().await.map_err(map_error)?;
        Ok(())
    }

    async fn list_price_sources(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<PriceSource>, IndustryError> {
        let ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM price_sources WHERE workspace_id = $1 ORDER BY updated_at DESC",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        let mut sources = Vec::with_capacity(ids.len());
        for id in ids {
            sources.push(
                self.load_price_source(workspace_id, PriceSourceId(id))
                    .await?,
            );
        }
        Ok(sources)
    }

    async fn get_blueprint_observation(
        &self,
        workspace_id: WorkspaceId,
        observation_id: Uuid,
    ) -> Result<BlueprintObservation, IndustryError> {
        load_blueprint_observation(&self.pool, workspace_id, observation_id).await
    }

    async fn list_blueprint_observations(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        blueprint_type_id: i64,
    ) -> Result<Vec<BlueprintObservation>, IndustryError> {
        sqlx::query_as::<_, BlueprintObservationRow>(
            r#"SELECT bo.id,bo.workspace_id,bo.owner_id,bo.eve_item_id,bo.blueprint_type_id,
                      COALESCE(t.name_en,bo.captured_blueprint_name) AS captured_blueprint_name,
                      bo.blueprint_kind,bo.material_efficiency,
                      bo.time_efficiency,bo.licensed_runs,bo.location_id,bo.location_flag,
                      COALESCE(bo.captured_location_name,mln.location_name) AS captured_location_name,
                      bo.observed_at,bo.imported_at,
                      o.display_name AS owner_name
               FROM blueprint_observations bo
               JOIN owners o ON o.id = bo.owner_id
               LEFT JOIN market_location_names mln
                 ON mln.workspace_id=bo.workspace_id AND mln.location_id=bo.location_id
               LEFT JOIN sde_imports si ON si.active
               LEFT JOIN sde_types t ON t.import_id=si.id AND t.type_id=bo.blueprint_type_id
               WHERE bo.workspace_id=$1 AND bo.owner_id=$2 AND bo.blueprint_type_id=$3
               ORDER BY (bo.blueprint_kind='original' OR bo.licensed_runs IS NOT NULL) DESC,
                        bo.material_efficiency DESC, bo.time_efficiency DESC,
                        bo.observed_at DESC, bo.eve_item_id"#,
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(blueprint_type_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(BlueprintObservationRow::into_domain)
        .collect()
    }

    async fn get_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
    ) -> Result<PriceSource, IndustryError> {
        self.load_price_source(workspace_id, source_id).await
    }

    /// Resolves prices directly against `scope` -- no
    /// `PriceSource` involved, reading through `scoped_order_books`, the
    /// same merged ESI+import read path the Market Browser uses. Freshness
    /// and coverage are global constants rather than per-source config:
    /// partial coverage never hard-errors, it's simply the caller's
    /// `missing` state to handle, matching how every other market-derived
    /// read in this codebase treats an incomplete book as data, not a failure
    /// -- `require_full_coverage: false` on the shared resolver
    /// is exactly that: a partially-covered request still prices off its
    /// best-available depth rather than being dropped.
    async fn derive_market_price_items(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
        requests: Vec<MarketPriceRequest>,
        evidence: Option<&iskworks_core::MarketScopeEvidence>,
    ) -> Result<Vec<PriceSourceItem>, IndustryError> {
        const FRESH_AFTER_HOURS: u32 = 1;
        const STALE_AFTER_HOURS: u32 = 24;

        let market = PgMarketRepository::new(self.pool.clone());
        // Pinned to the graph request's frozen evidence when supplied: one
        // `as_of` for every node's freshness/staleness classification, order
        // books cut off at that instant, and the import batch fixed -- so a
        // refresh completing mid-projection cannot move a later node's price.
        let now = evidence.map_or_else(Utc::now, |evidence| evidence.as_of);
        let observed_cutoff = evidence.map(|evidence| evidence.as_of);
        let import_batch_id = evidence.and_then(|evidence| evidence.import_batch_id);
        let type_ids: Vec<i64> = requests.iter().map(|request| request.type_id).collect();
        let books = market
            .scoped_order_books_as_of(
                workspace_id,
                scope,
                &type_ids,
                observed_cutoff,
                import_batch_id,
            )
            .await?;
        let resolution = resolve_market_price_items(
            &requests,
            |type_id| books.get(&type_id).map(Vec::as_slice),
            now,
            FRESH_AFTER_HOURS,
            STALE_AFTER_HOURS,
            false,
        )?;
        Ok(resolution.items)
    }

    async fn resolve_market_evidence(
        &self,
        workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
    ) -> Result<iskworks_core::MarketScopeEvidence, IndustryError> {
        let market = PgMarketRepository::new(self.pool.clone());
        Ok(market.resolve_scope_evidence(workspace_id, scope).await?)
    }

    async fn create_price_source(&self, source: PriceSource) -> Result<PriceSource, IndustryError> {
        sqlx::query(
            r#"
            INSERT INTO price_sources (
              id, workspace_id, display_name, description, source_kind,
              revision, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, 'manual', 1, $5, $5)
            "#,
        )
        .bind(source.id.0)
        .bind(source.workspace_id.0)
        .bind(&source.name)
        .bind(&source.description)
        .bind(source.created_at)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        self.load_price_source(source.workspace_id, source.id).await
    }

    async fn update_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        command: UpdatePriceSourceCommand,
    ) -> Result<PriceSource, IndustryError> {
        let result = sqlx::query(
            r#"
            UPDATE price_sources SET display_name = $1, description = $2,
              revision = revision + 1, updated_at = $3
            WHERE workspace_id = $4 AND id = $5 AND revision = $6
            "#,
        )
        .bind(command.name.trim())
        .bind(command.description.trim())
        .bind(crate::db_now())
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(i64_from_u64(command.expected_revision)?)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        classify_source_result(&self.pool, workspace_id, source_id, result.rows_affected()).await?;
        self.load_price_source(workspace_id, source_id).await
    }

    async fn upsert_price_items(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        expected_revision: u64,
        items: Vec<PriceSourceItem>,
    ) -> Result<PriceSource, IndustryError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        lock_source(&mut tx, workspace_id, source_id, expected_revision).await?;
        for item in items {
            sqlx::query(
                r#"
                INSERT INTO price_source_items (
                  price_source_id, type_id, captured_name, price, note, updated_at
                ) VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (price_source_id, type_id) DO UPDATE SET
                  captured_name = EXCLUDED.captured_name,
                  price = EXCLUDED.price,
                  note = EXCLUDED.note,
                  updated_at = EXCLUDED.updated_at
                "#,
            )
            .bind(source_id.0)
            .bind(item.type_id)
            .bind(item.type_name)
            .bind(item.price.0)
            .bind(item.note)
            .bind(item.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        }
        bump_source(&mut tx, source_id).await?;
        tx.commit().await.map_err(map_error)?;
        self.load_price_source(workspace_id, source_id).await
    }

    async fn remove_price_item(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        type_id: i64,
        expected_revision: u64,
    ) -> Result<PriceSource, IndustryError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        lock_source(&mut tx, workspace_id, source_id, expected_revision).await?;
        sqlx::query("DELETE FROM price_source_items WHERE price_source_id = $1 AND type_id = $2")
            .bind(source_id.0)
            .bind(type_id)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        bump_source(&mut tx, source_id).await?;
        tx.commit().await.map_err(map_error)?;
        self.load_price_source(workspace_id, source_id).await
    }

    async fn delete_price_source(
        &self,
        workspace_id: WorkspaceId,
        source_id: PriceSourceId,
        expected_revision: u64,
    ) -> Result<(), IndustryError> {
        let result = sqlx::query(
            "DELETE FROM price_sources WHERE workspace_id = $1 AND id = $2 AND revision = $3",
        )
        .bind(workspace_id.0)
        .bind(source_id.0)
        .bind(i64_from_u64(expected_revision)?)
        .execute(&self.pool)
        .await
        .map_err(map_error)?;
        classify_source_result(&self.pool, workspace_id, source_id, result.rows_affected()).await
    }

    async fn get_facility_profile(
        &self,
        workspace_id: WorkspaceId,
        facility_id: iskworks_core::FacilityProfileId,
    ) -> Result<iskworks_core::IndustryFacilityProfile, IndustryError> {
        crate::facility::load_profile(&self.pool, workspace_id, facility_id)
            .await
            .map_err(IndustryError::Facility)
    }
}

/// Inserts a Build row with its captured recipe lines and draft planning.
async fn insert_build(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    build: &Build,
) -> Result<(), IndustryError> {
    let product = build.recipe.primary_product();
    sqlx::query(
        r#"
        INSERT INTO builds (
          id, workspace_id, owner_id, display_name, recipe_kind,
          blueprint_type_id, blueprint_name, reaction_formula_type_id, reaction_formula_name,
          product_type_id, product_name, product_quantity_per_run,
          duration_seconds_per_run, source_sde_dataset_id, source_sde_version,
          recipe_fingerprint, runs, notes, revision, created_at, updated_at,
          plan_root_build_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                $13, $14, $15, $16, $17, $18, 1, $19, $19, $1)
        "#,
    )
    .bind(build.id.0)
    .bind(build.workspace_id.0)
    .bind(build.owner_id.0)
    .bind(&build.name)
    .bind(recipe_kind_str(build.recipe.kind()))
    .bind(build.recipe.blueprint_type_id())
    .bind(build.recipe.name_if_manufacturing())
    .bind(build.recipe.reaction_formula_type_id())
    .bind(build.recipe.name_if_reaction())
    .bind(product.type_id)
    .bind(&product.type_name)
    .bind(i64_from_u64(product.quantity_per_run)?)
    .bind(
        build
            .recipe
            .duration_seconds_per_run()
            .map(i64_from_u64)
            .transpose()?,
    )
    .bind(build.recipe.source_sde_dataset_id())
    .bind(build.recipe.source_sde_version())
    .bind(build.recipe.fingerprint())
    .bind(i64_from_u64(build.runs)?)
    .bind(&build.notes)
    .bind(build.created_at)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    insert_recipe_lines(
        tx,
        build.id,
        "build_recipe_materials",
        build.recipe.materials(),
    )
    .await?;
    insert_recipe_lines(
        tx,
        build.id,
        "build_recipe_products",
        build.recipe.products(),
    )
    .await?;
    write_draft_planning(tx, build.id, build.draft_planning.as_ref()).await?;
    Ok(())
}
