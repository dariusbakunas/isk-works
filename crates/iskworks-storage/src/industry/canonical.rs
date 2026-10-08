//! The transactional side of the canonical planner: plan locking and the
//! canonical consumer write. Every decision is computed purely in
//! `iskworks_core::canonical_planner` from a fresh bounded load; each
//! function here only re-verifies, under lock, that the plan is still in
//! exactly the state that decision was computed from, and then applies it.

use iskworks_core::canonical_planner::{
    CanonicalConsumerWrite, CanonicalWriteError, PlannedProducer,
};
use iskworks_core::plan_state::PlanBuildState;
use iskworks_core::production_dependency::{DependencySourcing, ProductionMethod};

use super::*;

/// Lock every Build of one root plan (and their drafts), in id order, and
/// return the plan state (`iskworks_core::plan_state::plan_state`
/// shape) as it is under the lock.
pub(super) async fn lock_plan_state(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    root: BuildId,
) -> Result<Vec<PlanBuildState>, IndustryError> {
    let builds: Vec<(Uuid, i64)> = sqlx::query_as(
        r#"
        SELECT id, revision FROM builds
        WHERE workspace_id = $1 AND (id = $2 OR plan_root_build_id = $2)
        ORDER BY id
        FOR UPDATE
        "#,
    )
    .bind(workspace_id.0)
    .bind(root.0)
    .fetch_all(&mut **tx)
    .await
    .map_err(map_error)?;
    let ids: Vec<Uuid> = builds.iter().map(|(id, _)| *id).collect();
    let drafts: std::collections::HashMap<Uuid, DateTime<Utc>> =
        sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
            "SELECT build_id, updated_at FROM build_draft_planning WHERE build_id = ANY($1) \
             ORDER BY build_id FOR UPDATE",
        )
        .bind(&ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(map_error)?
        .into_iter()
        .collect();
    builds
        .iter()
        .map(|(id, revision)| {
            Ok(PlanBuildState {
                build_id: BuildId(*id),
                revision: u64_from_i64(*revision)?,
                draft_updated_at: drafts.get(id).copied(),
            })
        })
        .collect()
}

/// Defence in depth for every canonical write: walk the plan's producer
/// graph from the root inside the transaction and fail on any cycle.
async fn assert_canonical_plan_acyclic(
    tx: &mut Transaction<'_, Postgres>,
    root: BuildId,
) -> Result<(), IndustryError> {
    let cyclic: bool = sqlx::query_scalar(
        r#"
        WITH RECURSIVE walk AS (
            SELECT $1::uuid AS id
          UNION ALL
            SELECT d.producer_build_id
            FROM production_dependencies d
            JOIN walk ON d.consumer_build_id = walk.id
            WHERE d.plan_root_build_id = $1 AND d.sourcing = 'produce'
              AND d.producer_build_id IS NOT NULL
        )
        CYCLE id SET is_cycle USING path
        SELECT EXISTS (SELECT 1 FROM walk WHERE is_cycle)
        "#,
    )
    .bind(root.0)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_error)?;
    if cyclic {
        return Err(CanonicalWriteError::CanonicalGraphCorrupt {
            detail: "the write would make the root plan's producer graph cyclic".to_string(),
        }
        .into());
    }
    Ok(())
}

fn method_of(sourcing: DependencySourcing) -> (Option<&'static str>, Option<i64>) {
    match sourcing {
        DependencySourcing::Buy => (None, None),
        DependencySourcing::Produce { method } => {
            let (kind, type_id) = method_columns(method);
            (Some(kind), Some(type_id))
        }
    }
}

fn scope_str(scope: iskworks_core::FulfillmentScope) -> &'static str {
    match scope {
        iskworks_core::FulfillmentScope::Missing => "missing",
        iskworks_core::FulfillmentScope::Full => "full",
    }
}

/// Insert one canonical producer Build (plan root set explicitly) and one
/// Buy/Missing demand edge per distinct recipe material -- the same default
/// sourcing a freshly created Build starts with.
async fn insert_canonical_producer(
    tx: &mut Transaction<'_, Postgres>,
    root: BuildId,
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
                $13, $14, $15, $16, $17, $18, 1, $19, $19, $20)
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
    .bind(root.0)
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
    insert_buy_edges(tx, root, build.id).await
}

/// One Buy/Missing edge per distinct recipe material of `consumer` that has
/// none yet.
pub(super) async fn insert_buy_edges(
    tx: &mut Transaction<'_, Postgres>,
    root: BuildId,
    consumer: BuildId,
) -> Result<(), IndustryError> {
    sqlx::query(
        r#"
        INSERT INTO production_dependencies (
          workspace_id, plan_root_build_id, consumer_build_id, component_type_id,
          sourcing, fulfillment_scope
        )
        SELECT DISTINCT b.workspace_id, $1::uuid, b.id, m.type_id, 'buy', 'missing'
        FROM builds b
        JOIN build_recipe_materials m ON m.build_id = b.id
        WHERE b.id = $2
        ON CONFLICT (consumer_build_id, component_type_id) DO NOTHING
        "#,
    )
    .bind(root.0)
    .bind(consumer.0)
    .execute(&mut **tx)
    .await
    .map_err(map_error)?;
    Ok(())
}

pub(super) async fn apply_canonical_consumer_write(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    write: CanonicalConsumerWrite,
) -> Result<(), IndustryError> {
    let mut tx = pool.begin().await.map_err(map_error)?;
    let current = lock_plan_state(&mut tx, workspace_id, write.root).await?;
    if current != write.expected_plan_state {
        // Distinguish the consumer's own stale revision (the caller's
        // optimistic token) from a concurrent change elsewhere in the plan.
        let consumer_revision = current
            .iter()
            .find(|state| state.build_id == write.consumer)
            .map(|state| state.revision);
        return Err(match consumer_revision {
            None => IndustryError::BuildNotFound,
            Some(revision) if revision != write.update.expected_revision => {
                IndustryError::RevisionConflict
            }
            Some(_) => CanonicalWriteError::PlanChanged.into(),
        });
    }

    // (1) The consumer's own update, revision-checked.
    let update = &write.update;
    let result = if let Some(recipe) = &update.replacement_recipe {
        let product = recipe.primary_product();
        sqlx::query(
            r#"
            UPDATE builds SET display_name = $1, runs = $2, notes = $3,
              recipe_kind = $4, blueprint_type_id = $5, blueprint_name = $6,
              reaction_formula_type_id = $7, reaction_formula_name = $8,
              product_type_id = $9, product_name = $10, product_quantity_per_run = $11,
              duration_seconds_per_run = $12, source_sde_dataset_id = $13,
              source_sde_version = $14, recipe_fingerprint = $15,
              revision = revision + 1, updated_at = $16
            WHERE workspace_id = $17 AND id = $18 AND revision = $19
            "#,
        )
        .bind(&update.name)
        .bind(i64_from_u64(update.runs)?)
        .bind(&update.notes)
        .bind(recipe_kind_str(recipe.kind()))
        .bind(recipe.blueprint_type_id())
        .bind(recipe.name_if_manufacturing())
        .bind(recipe.reaction_formula_type_id())
        .bind(recipe.name_if_reaction())
        .bind(product.type_id)
        .bind(&product.type_name)
        .bind(i64_from_u64(product.quantity_per_run)?)
        .bind(
            recipe
                .duration_seconds_per_run()
                .map(i64_from_u64)
                .transpose()?,
        )
        .bind(recipe.source_sde_dataset_id())
        .bind(recipe.source_sde_version())
        .bind(recipe.fingerprint())
        .bind(crate::db_now())
        .bind(workspace_id.0)
        .bind(write.consumer.0)
        .bind(i64_from_u64(update.expected_revision)?)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?
    } else {
        sqlx::query(
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
        .bind(write.consumer.0)
        .bind(i64_from_u64(update.expected_revision)?)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?
    };
    if result.rows_affected() == 0 {
        return Err(classify_build_update(&mut tx, workspace_id, write.consumer).await?);
    }
    if let Some(recipe) = &update.replacement_recipe {
        sqlx::query("DELETE FROM build_recipe_materials WHERE build_id = $1")
            .bind(write.consumer.0)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        sqlx::query("DELETE FROM build_recipe_products WHERE build_id = $1")
            .bind(write.consumer.0)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        insert_recipe_lines(
            &mut tx,
            write.consumer,
            "build_recipe_materials",
            recipe.materials(),
        )
        .await?;
        insert_recipe_lines(
            &mut tx,
            write.consumer,
            "build_recipe_products",
            recipe.products(),
        )
        .await?;
        // The consumer's requirement set follows its recipe: removed
        // components lose their edge (a producer is only detached, never
        // deleted), new ones start as Buy.
        sqlx::query(
            r#"
            DELETE FROM production_dependencies d
            WHERE d.consumer_build_id = $1
              AND NOT EXISTS (
                SELECT 1 FROM build_recipe_materials m
                WHERE m.build_id = $1 AND m.type_id = d.component_type_id
              )
            "#,
        )
        .bind(write.consumer.0)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        insert_buy_edges(&mut tx, write.root, write.consumer).await?;
    }
    write_draft_planning(&mut tx, write.consumer, update.draft_planning.as_ref()).await?;

    // (2) New producers -- exactly one per identity.
    let mut created: std::collections::BTreeMap<
        iskworks_core::production_dependency::CanonicalProducerKey,
        BuildId,
    > = std::collections::BTreeMap::new();
    for producer in &write.new_producers {
        if created.contains_key(&producer.key) {
            continue;
        }
        insert_canonical_producer(&mut tx, write.root, &producer.build).await?;
        created.insert(producer.key, producer.build.id);
    }

    // (3) Edge writes.
    for edge in &write.edge_writes {
        let producer = match edge.producer {
            PlannedProducer::None => None,
            PlannedProducer::Existing { producer } => Some(producer),
            PlannedProducer::Create { key } => Some(*created.get(&key).ok_or_else(|| {
                IndustryError::Persistence(format!("no producer was prepared for {key:?}"))
            })?),
        };
        let (method_kind, method_type_id) = method_of(edge.sourcing);
        let result = sqlx::query(
            r#"
            UPDATE production_dependencies
            SET sourcing = $1, method_kind = $2, method_type_id = $3,
                producer_build_id = $4, fulfillment_scope = $5,
                revision = revision + 1, updated_at = now()
            WHERE id = $6 AND consumer_build_id = $7 AND plan_root_build_id = $8
            "#,
        )
        .bind(match edge.sourcing {
            DependencySourcing::Buy => "buy",
            DependencySourcing::Produce { .. } => "produce",
        })
        .bind(method_kind)
        .bind(method_type_id)
        .bind(producer.map(|id| id.0))
        .bind(scope_str(edge.fulfillment_scope))
        .bind(edge.dependency_id)
        .bind(write.consumer.0)
        .bind(write.root.0)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if is_check_violation(&error) {
                IndustryError::CanonicalWrite(CanonicalWriteError::CanonicalGraphCorrupt {
                    detail: error.to_string(),
                })
            } else {
                map_error(error)
            }
        })?;
        if result.rows_affected() != 1 {
            return Err(CanonicalWriteError::PlanChanged.into());
        }
    }

    assert_canonical_plan_acyclic(&mut tx, write.root).await?;
    tx.commit().await.map_err(map_error)?;
    Ok(())
}

fn is_check_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database) if database.code().as_deref() == Some("23514"))
}

/// Sourcing guard for the plain `update_draft` path (settings patches,
/// descendant configuration): it writes no demand edges, so it may not
/// change a Build's sourcing (resolutions / scopes) or recipe -- those go
/// through the canonical consumer write.
pub(super) async fn guard_sourcing_unchanged(
    tx: &mut Transaction<'_, Postgres>,
    build_id: BuildId,
    update: &DraftUpdate,
) -> Result<(), IndustryError> {
    let refuse = || -> IndustryError {
        CanonicalWriteError::CanonicalWriteRequired { build: build_id }.into()
    };
    if update.replacement_recipe.is_some() {
        return Err(refuse());
    }
    let stored: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT planning_input FROM build_draft_planning WHERE build_id = $1")
            .bind(build_id.0)
            .fetch_optional(&mut **tx)
            .await
            .map_err(map_error)?;
    let sourcing_of = |input: Option<&DraftPlanningInput>| {
        let mut resolutions: Vec<(i64, ProductionMethod)> = input
            .map(|input| {
                input
                    .component_resolutions
                    .iter()
                    .map(|resolution| (resolution.type_id, resolution.recipe.into()))
                    .collect()
            })
            .unwrap_or_default();
        resolutions.sort();
        let mut full: Vec<i64> = input
            .map(|input| {
                input
                    .fulfillment_scopes
                    .iter()
                    .filter(|scope| scope.scope == iskworks_core::FulfillmentScope::Full)
                    .map(|scope| scope.type_id)
                    .collect()
            })
            .unwrap_or_default();
        full.sort_unstable();
        (resolutions, full)
    };
    let stored: Option<DraftPlanningInput> = stored
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| IndustryError::Persistence(format!("invalid stored draft: {error}")))?;
    let new = update
        .draft_planning
        .as_ref()
        .map(|snapshot| &snapshot.input);
    if sourcing_of(stored.as_ref()) != sourcing_of(new) {
        return Err(refuse());
    }
    Ok(())
}
