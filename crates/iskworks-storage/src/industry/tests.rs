use std::sync::Arc;

use super::*;
use crate::PgOrderRepository;
use iskworks_core::{
    order::{OrderId, OrderRepository, PlanOperationEvidence},
    CapturedReactionFormula, IndustryService, MarketPricingPolicy, NewBuild, OwnerId,
    RecipeCurrency,
};

async fn fixture_workspace(pool: &PgPool) -> (WorkspaceId, OwnerId, Uuid) {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = OwnerId(Uuid::new_v4());
    let import_id = Uuid::new_v4();
    let now = Utc::now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
            "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) VALUES ($1, 'Reaction Build Test', $2, $3, $3)",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
            "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) VALUES ($1, $2, 'manual', 'Reaction Build Test', false, $3, $3)",
        )
        .bind(owner_id.0)
        .bind(workspace_id.0)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
            "INSERT INTO sde_imports (id, source_version, source_label, source_checksum, status, active, started_at, completed_at) VALUES ($1, 'test', 'fixture', $2, 'active', true, $3, $3)",
        )
        .bind(import_id)
        .bind(format!("reaction-build-{}", Uuid::new_v4()))
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (workspace_id, owner_id, import_id)
}

fn reaction_recipe(import_id: Uuid) -> BuildRecipe {
    BuildRecipe::Reaction(
        CapturedReactionFormula::capture(
            import_id,
            "test".to_string(),
            iskworks_sde::ReactionFormulaRecipe {
                reaction_formula_type_id: 46_157,
                reaction_formula_name: "Methanofullerene Reaction Formula".to_string(),
                duration_seconds: Some(10_800),
                materials: vec![iskworks_sde::RecipeLine {
                    type_id: 37,
                    type_name: "Isogen".to_string(),
                    quantity: 300,
                }],
                products: vec![iskworks_sde::RecipeLine {
                    type_id: 30_306,
                    type_name: "Methanofullerene".to_string(),
                    quantity: 160,
                }],
            },
        )
        .unwrap(),
    )
}

fn manufacturing_recipe(import_id: Uuid) -> BuildRecipe {
    BuildRecipe::Manufacturing(CapturedRecipe {
        source_sde_dataset_id: import_id,
        source_sde_version: "test".to_string(),
        blueprint_type_id: 6_830,
        blueprint_name: "Rifter Blueprint".to_string(),
        duration_seconds_per_run: Some(600),
        materials: vec![CapturedRecipeLine {
            type_id: 34,
            type_name: "Tritanium".to_string(),
            quantity_per_run: 1_000,
            sort_order: 0,
        }],
        products: vec![CapturedRecipeLine {
            type_id: 5_876,
            type_name: "Rifter".to_string(),
            quantity_per_run: 1,
            sort_order: 0,
        }],
        fingerprint: "manufacturing-fixture".to_string(),
    })
}

fn draft_build(workspace_id: WorkspaceId, owner_id: OwnerId, recipe: BuildRecipe) -> Build {
    let now = Utc::now();
    Build {
        id: BuildId::new(),
        workspace_id,
        owner_id,
        name: "Test Build".to_string(),
        recipe,
        runs: 1,
        notes: String::new(),
        revision: 1,
        created_at: now,
        updated_at: now,
        draft_planning: None,
        recipe_currency: RecipeCurrency::Current,
        active_sde_version: None,
        product_category_name: None,
        product_group_name: None,
        selected_blueprint_origin: None,
        has_owned_blueprint: false,
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reaction_build_round_trips_through_create_and_load(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());

    let build = repository
        .create_build(NewBuild {
            build: draft_build(workspace_id, owner_id, reaction_recipe(import_id)),
        })
        .await
        .unwrap();

    assert!(matches!(build.recipe, BuildRecipe::Reaction(_)));
    assert_eq!(build.recipe.reaction_formula_type_id(), Some(46_157));
    assert_eq!(build.recipe.blueprint_type_id(), None);

    let (recipe_kind, blueprint_type_id): (String, Option<i64>) =
        sqlx::query_as("SELECT recipe_kind, blueprint_type_id FROM builds WHERE id = $1")
            .bind(build.id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recipe_kind, "reaction");
    assert_eq!(blueprint_type_id, None);
}

/// A new root is its own plan the moment it's written, with no cutover
/// (no `canonical_planner_cutovers` row).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_new_root_build_is_born_a_canonical_plan(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());

    let build = repository
        .create_root_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();

    let plan_root: Uuid = sqlx::query_scalar("SELECT plan_root_build_id FROM builds WHERE id = $1")
        .bind(build.id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(plan_root, build.id.0);
    let cutovers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM canonical_planner_cutovers WHERE plan_root_build_id = $1",
    )
    .bind(build.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cutovers, 0);
    // Its initial demand edges exist (one per material), so the first
    // canonical sourcing write has rows to address.
    let edges: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM production_dependencies WHERE consumer_build_id = $1",
    )
    .bind(build.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(edges > 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn manufacturing_build_still_round_trips(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());

    let build = repository
        .create_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();

    assert!(matches!(build.recipe, BuildRecipe::Manufacturing(_)));
    assert_eq!(build.recipe.blueprint_type_id(), Some(6_830));
    assert_eq!(build.recipe.reaction_formula_type_id(), None);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn builds_reject_a_mismatched_recipe_identity(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let now = Utc::now();

    // recipe_kind says reaction, but a blueprint identity is populated
    // too -- the builds_recipe_identity_matches_kind CHECK must reject
    // this.
    let result = sqlx::query(
        r#"
            INSERT INTO builds (
              id, workspace_id, owner_id, display_name, recipe_kind,
              blueprint_type_id, blueprint_name, reaction_formula_type_id, reaction_formula_name,
              product_type_id, product_name, product_quantity_per_run,
              source_sde_dataset_id, source_sde_version, recipe_fingerprint, runs, notes,
              revision, created_at, updated_at, plan_root_build_id
            ) VALUES ($1,$2,$3,'Bad Build','reaction',6830,'Rifter Blueprint',
              46157,'Methanofullerene Reaction Formula',30306,'Methanofullerene',160,
              $4,'test','bad-fingerprint',1,'',1,$5,$5,$1)
            "#,
    )
    .bind(Uuid::new_v4())
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(import_id)
    .bind(now)
    .execute(&pool)
    .await;

    assert!(result.is_err());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn switching_recipe_clears_the_other_identity_pair(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());

    let build = repository
        .create_root_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();
    // A recipe change is a sourcing change, so it goes through the
    // canonical consumer write.
    let records = repository
        .load_root_plan(workspace_id, build.id)
        .await
        .unwrap();
    let updated = repository
        .apply_canonical_consumer_write(
            workspace_id,
            iskworks_core::canonical_planner::CanonicalConsumerWrite {
                root: build.id,
                expected_plan_state: iskworks_core::plan_state::plan_state(&records),
                consumer: build.id,
                update: DraftUpdate {
                    expected_revision: build.revision,
                    name: build.name.clone(),
                    runs: build.runs,
                    notes: build.notes.clone(),
                    replacement_recipe: Some(reaction_recipe(import_id)),
                    draft_planning: None,
                },
                new_producers: Vec::new(),
                edge_writes: Vec::new(),
            },
        )
        .await
        .unwrap();

    assert!(matches!(updated.recipe, BuildRecipe::Reaction(_)));

    let (recipe_kind, blueprint_type_id, blueprint_name): (String, Option<i64>, Option<String>) =
        sqlx::query_as(
            "SELECT recipe_kind, blueprint_type_id, blueprint_name FROM builds WHERE id = $1",
        )
        .bind(build.id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(recipe_kind, "reaction");
    assert_eq!(blueprint_type_id, None);
    assert_eq!(blueprint_name, None);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_builds_lists_plan_roots_but_not_their_producers(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());

    let root = repository
        .create_root_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();
    let producer = repository
        .create_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();
    sqlx::query("UPDATE builds SET plan_root_build_id = $1 WHERE id = $2")
        .bind(root.id.0)
        .bind(producer.id.0)
        .execute(&pool)
        .await
        .unwrap();

    let listed = repository.list_builds(workspace_id).await.unwrap();
    assert!(listed.iter().any(|build| build.id == root.id));
    assert!(!listed.iter().any(|build| build.id == producer.id));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_root_deletes_its_plan_producers_and_their_snapshots(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());

    let root = repository
        .create_root_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();
    // A producer of the root's plan: no parent link, just its plan root.
    let producer = repository
        .create_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();
    sqlx::query("UPDATE builds SET plan_root_build_id = $1 WHERE id = $2")
        .bind(root.id.0)
        .bind(producer.id.0)
        .execute(&pool)
        .await
        .unwrap();
    // A Build-only price snapshot of the producer (no Epic references it).
    let snapshot_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO price_snapshots (id, workspace_id, build_id, captured_source_name, captured_source_revision, purpose, created_at) \
         VALUES ($1, $2, $3, 'Market', 1, 'build_planning', $4)",
    )
    .bind(snapshot_id)
    .bind(workspace_id.0)
    .bind(producer.id.0)
    .bind(Utc::now())
    .execute(&pool)
    .await
    .unwrap();

    repository
        .delete_build(workspace_id, root.id, root.revision, false)
        .await
        .unwrap();

    let result = repository.get_build(workspace_id, producer.id).await;
    assert!(matches!(result, Err(IndustryError::BuildNotFound)));
    let snapshots: i64 = sqlx::query_scalar("SELECT count(*) FROM price_snapshots WHERE id = $1")
        .bind(snapshot_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        snapshots, 0,
        "the producer's Build-only snapshot goes with it"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn deletion_ownership_build_delete_preserves_epic_ticket_and_frozen_economics(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());

    let build = repository
        .create_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();

    // An Epic and a production Ticket captured this Build. They are frozen
    // workflow snapshots, not children of the live planning object.
    let snapshot_id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO price_snapshots (id, workspace_id, build_id, captured_source_name, captured_source_revision, purpose, created_at) \
         VALUES ($1, $2, $3, 'Market', 0, 'build_planning', $4)",
    )
    .bind(snapshot_id)
    .bind(workspace_id.0)
    .bind(build.id.0)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    let order_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orders (id, workspace_id, owner_id, source_build_id, source_build_revision, display_name, runs, recipe_fingerprint, price_snapshot_id, estimated_material_cost, missing_price_count, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, 'Order', 1, 'fp', $6, 0, 0, $7, $7)",
    )
    .bind(order_id)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(build.id.0)
    .bind(i64::try_from(build.revision).unwrap())
    .bind(snapshot_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    let operation_id = Uuid::new_v4();
    let evidence = PlanOperationEvidence {
        effective_me: Some(7),
        effective_te: Some(12),
        job_count: 1,
        recipe_currency: RecipeCurrency::Current,
        installation: None,
        warnings: vec!["captured warning".to_string()],
    };
    sqlx::query(
        "INSERT INTO order_plan_operations (id, order_id, occurrence_key, parent_occurrence_key, build_id, activity, runs, persisted_runs, product_type_id, product_name, output_per_run, produced_quantity, blueprint_or_formula_type_id, material_component_cost, own_installation_cost, total_production_cost, complete, evidence, created_at) \
         VALUES ($1, $2, $3, NULL, $4, 'manufacturing', 1, 1, 5876, 'Rifter', 1, 1, 6830, 42, 8, 50, true, $5, $6)",
    )
    .bind(operation_id)
    .bind(order_id)
    .bind(format!("root:{}", build.id.0))
    .bind(build.id.0)
    .bind(serde_json::to_value(&evidence).unwrap())
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    let ticket_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tickets (id, workspace_id, owner_id, display_id, kind, type_id, captured_name, quantity, order_id, source_build_id, notes, status, created_at, updated_at) \
         VALUES ($1, $2, $3, 'ISK-DELETE-BUILD', 'manufacturing', 5876, 'Rifter', 1, $4, $5, '', 'complete', $6, $6)",
    )
    .bind(ticket_id)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(order_id)
    .bind(build.id.0)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    repository
        .delete_build(workspace_id, build.id, build.revision, false)
        .await
        .unwrap();
    assert!(matches!(
        repository.get_build(workspace_id, build.id).await,
        Err(IndustryError::BuildNotFound)
    ));
    let epic: (Option<Uuid>, Decimal, Uuid) = sqlx::query_as(
        "SELECT source_build_id, estimated_material_cost, price_snapshot_id FROM orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(epic, (None, Decimal::ZERO, snapshot_id));
    let ticket: (Option<Uuid>, Option<Uuid>, String) =
        sqlx::query_as("SELECT source_build_id, order_id, status FROM tickets WHERE id = $1")
            .bind(ticket_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ticket, (None, Some(order_id), "complete".to_string()));
    let retained_snapshot: (Option<Uuid>, String) =
        sqlx::query_as("SELECT build_id, captured_source_name FROM price_snapshots WHERE id = $1")
            .bind(snapshot_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(retained_snapshot, (None, "Market".to_string()));
    let order_repository = PgOrderRepository::new(pool.clone());
    let detached_order = order_repository
        .get_order(workspace_id, OrderId(order_id))
        .await
        .unwrap();
    assert_eq!(detached_order.source_build_id, None);
    assert_eq!(detached_order.estimated_material_cost, Money::zero());
    let detached_operations = order_repository
        .list_order_plan_operations(OrderId(order_id))
        .await
        .unwrap();
    assert_eq!(detached_operations.len(), 1);
    assert_eq!(detached_operations[0].build_id, None);
    assert_eq!(
        detached_operations[0].material_component_cost,
        Some(Money(Decimal::from(42)))
    );
    assert_eq!(detached_operations[0].evidence, evidence);
    let orders_after: i64 = sqlx::query_scalar("SELECT count(*) FROM orders WHERE id = $1")
        .bind(order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(orders_after, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn blueprint_observation_names_are_resolved_live_even_when_the_captured_name_was_a_placeholder(
    pool: PgPool,
) {
    // Simulates the race between an ESI blueprint sync and SDE import
    // activation: the sync ran before the SDE import went active, so it
    // fell back to storing a placeholder name. The read path should
    // still show the real name once the SDE import (or a later one) has
    // the type, without needing a re-sync to fix the stored row.
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());
    let observation_id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
            "INSERT INTO sde_types (import_id, type_id, name_en, published) VALUES ($1, 12006, 'Ishtar Blueprint', true)",
        )
        .bind(import_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
            r#"INSERT INTO blueprint_observations (
                 id, workspace_id, owner_id, connection_id, eve_item_id, blueprint_type_id,
                 captured_blueprint_name, blueprint_kind, material_efficiency, time_efficiency,
                 licensed_runs, location_id, location_flag, captured_location_name, observed_at,
                 source_payload, source_checksum
               ) VALUES ($1,$2,$3,NULL,$4,12006,'Unknown EVE type 12006','copy',5,10,1,60003760,'Hangar',NULL,$5,'{}','checksum')"#,
        )
        .bind(observation_id)
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(1_000_000_000_i64)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

    let listed = repository
        .list_blueprint_observations(workspace_id, owner_id, 12_006)
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].blueprint_name, "Ishtar Blueprint");

    let single = repository
        .get_blueprint_observation(workspace_id, observation_id)
        .await
        .unwrap();
    assert_eq!(single.blueprint_name, "Ishtar Blueprint");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn draft_planning_round_trips_fulfillment_scope_overrides(pool: PgPool) {
    use iskworks_core::{FulfillmentScope, FulfillmentScopeOverride};

    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());

    let build = repository
        .create_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();

    let fulfillment_scopes = vec![FulfillmentScopeOverride {
        type_id: 90_001,
        scope: FulfillmentScope::Missing,
    }];
    let draft_planning = DraftPlanningSnapshot {
        input: DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: None,
            expected_manual_price_list_revision: None,
            material_pricing_policy: MarketPricingPolicy::HighestBuy,
            output_pricing_policy: MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: None,
            manufacturing_facility: None,
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: Vec::new(),
            fulfillment_scopes: fulfillment_scopes.clone(),
        },
        updated_at: Utc::now(),
    };

    let updated = repository
        .update_draft(
            workspace_id,
            build.id,
            DraftUpdate {
                expected_revision: build.revision,
                name: build.name.clone(),
                runs: build.runs,
                notes: build.notes.clone(),
                replacement_recipe: None,
                draft_planning: Some(draft_planning),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        updated.draft_planning.unwrap().input.fulfillment_scopes,
        fulfillment_scopes
    );

    // Reload independently to prove this persisted, not just that the
    // update's own return value echoed what was sent.
    let reloaded = repository.get_build(workspace_id, build.id).await.unwrap();
    assert_eq!(
        reloaded.draft_planning.unwrap().input.fulfillment_scopes,
        fulfillment_scopes
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn load_build_resolves_output_classification_and_selected_blueprint_origin(pool: PgPool) {
    use iskworks_core::{BlueprintKind, BlueprintSelection};

    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());
    let now = Utc::now();

    // Classification chain for the Rifter (product type 5876): a "Frigate"
    // group under the "Ship" category.
    sqlx::query("INSERT INTO sde_categories (import_id, category_id, name_en, published) VALUES ($1, 6, 'Ship', true)")
        .bind(import_id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO sde_groups (import_id, group_id, name_en, category_id, published) VALUES ($1, 25, 'Frigate', 6, true)")
        .bind(import_id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO sde_types (import_id, type_id, name_en, group_id, published) VALUES ($1, 5876, 'Rifter', 25, true)")
        .bind(import_id).execute(&pool).await.unwrap();

    // An observed BPC the Build points at.
    let observation_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO blueprint_observations (
             id, workspace_id, owner_id, connection_id, eve_item_id, blueprint_type_id,
             captured_blueprint_name, blueprint_kind, material_efficiency, time_efficiency,
             licensed_runs, location_id, location_flag, captured_location_name, observed_at,
             source_payload, source_checksum
           ) VALUES ($1,$2,$3,NULL,$4,6830,'Rifter Blueprint','copy',5,10,25,60003760,'Hangar',NULL,$5,'{}','checksum')"#,
    )
    .bind(observation_id).bind(workspace_id.0).bind(owner_id.0).bind(2_000_000_000_i64).bind(now)
    .execute(&pool).await.unwrap();

    let build = repository
        .create_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();

    // No blueprint selected yet -> origin stays None; classification resolves;
    // the workspace still holds a current observation for blueprint 6830,
    // so `has_owned_blueprint` is true regardless of the selection.
    let loaded = repository.get_build(workspace_id, build.id).await.unwrap();
    assert_eq!(loaded.product_category_name.as_deref(), Some("Ship"));
    assert_eq!(loaded.product_group_name.as_deref(), Some("Frigate"));
    assert_eq!(loaded.selected_blueprint_origin, None);
    assert!(loaded.has_owned_blueprint);

    // Point the draft at the observed BPC, not yet captured (the legacy
    // sentinel) -> origin still resolves via the live fallback lookup.
    let with_observed = draft_planning_with_blueprint(BlueprintSelection::ObservedAsset {
        observation_id,
        kind: BlueprintKind::Unknown,
        material_efficiency: 0,
        time_efficiency: 0,
        licensed_runs: None,
    });
    let updated = repository
        .update_draft(
            workspace_id,
            build.id,
            DraftUpdate {
                expected_revision: loaded.revision,
                name: loaded.name.clone(),
                runs: loaded.runs,
                notes: loaded.notes.clone(),
                replacement_recipe: None,
                draft_planning: Some(with_observed),
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.selected_blueprint_origin, Some(BlueprintKind::Copy));

    // A manual BPO selection -> origin is Original.
    let with_manual = draft_planning_with_blueprint(BlueprintSelection::Manual {
        kind: BlueprintKind::Original,
        material_efficiency: 10,
        time_efficiency: 20,
        licensed_runs: None,
        notes: String::new(),
    });
    let manual = repository
        .update_draft(
            workspace_id,
            build.id,
            DraftUpdate {
                expected_revision: updated.revision,
                name: updated.name.clone(),
                runs: updated.runs,
                notes: updated.notes.clone(),
                replacement_recipe: None,
                draft_planning: Some(with_manual),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        manual.selected_blueprint_origin,
        Some(BlueprintKind::Original)
    );
}

// The observed-blueprint backfill, exercised against a real Postgres-persisted legacy
// row: a Build whose `draft_planning` JSONB carries the pre-migration
// `ObservedAsset` sentinel (only `observationId`, no captured kind/ME/TE --
// exactly what a pre-migration row contains).
// Idempotent by construction: the second run must do zero writes.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn backfill_captures_a_legacy_observed_asset_selection_and_is_idempotent(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());
    let now = Utc::now();

    let observation_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO blueprint_observations (
             id, workspace_id, owner_id, connection_id, eve_item_id, blueprint_type_id,
             captured_blueprint_name, blueprint_kind, material_efficiency, time_efficiency,
             licensed_runs, location_id, location_flag, captured_location_name, observed_at,
             source_payload, source_checksum
           ) VALUES ($1,$2,$3,NULL,$4,6830,'Rifter Blueprint','copy',10,20,300,60003760,'Hangar',NULL,$5,'{}','checksum')"#,
    )
    .bind(observation_id)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(2_000_000_000_i64)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let build = repository
        .create_root_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();
    let loaded = repository.get_build(workspace_id, build.id).await.unwrap();

    // The legacy sentinel: only `observationId` was ever persisted.
    let legacy = draft_planning_with_blueprint(BlueprintSelection::ObservedAsset {
        observation_id,
        kind: BlueprintKind::Unknown,
        material_efficiency: 0,
        time_efficiency: 0,
        licensed_runs: None,
    });
    repository
        .update_draft(
            workspace_id,
            build.id,
            DraftUpdate {
                expected_revision: loaded.revision,
                name: loaded.name.clone(),
                runs: loaded.runs,
                notes: loaded.notes.clone(),
                replacement_recipe: None,
                draft_planning: Some(legacy),
            },
        )
        .await
        .unwrap();

    let service = IndustryService::new(
        Arc::new(PgIndustryRepository::new(pool.clone())),
        Arc::new(crate::PgSdeRepository::new(pool.clone())),
    );

    let first_run = service
        .backfill_observed_blueprint_configurations(workspace_id)
        .await
        .unwrap();
    assert_eq!(first_run.builds_updated, 1);
    assert_eq!(first_run.selections_captured, 1);
    assert!(first_run.selections_unresolved.is_empty());

    let backfilled = repository.get_build(workspace_id, build.id).await.unwrap();
    match backfilled
        .draft_planning
        .unwrap()
        .input
        .blueprint_selection
        .unwrap()
    {
        BlueprintSelection::ObservedAsset {
            observation_id: captured_id,
            kind,
            material_efficiency,
            time_efficiency,
            ..
        } => {
            assert_eq!(captured_id, observation_id);
            assert_eq!(kind, BlueprintKind::Copy);
            assert_eq!(material_efficiency, 10);
            assert_eq!(time_efficiency, 20);
        }
        other => panic!("expected a captured ObservedAsset selection, got {other:?}"),
    }

    // Idempotent: rerunning against an already-migrated Build is a no-op.
    let second_run = service
        .backfill_observed_blueprint_configurations(workspace_id)
        .await
        .unwrap();
    assert_eq!(second_run.builds_updated, 0);
    assert_eq!(second_run.selections_captured, 0);
    assert!(second_run.selections_unresolved.is_empty());
}

/// Persists a root Build whose draft carries an already-captured
/// `ObservedAsset` copy selection (ME10/TE20) from before licensed runs were
/// frozen, naming an observation of `observed_me` with 300 licensed runs.
async fn captured_copy_without_licensed_runs(
    pool: &PgPool,
    observed_me: i16,
) -> (iskworks_core::WorkspaceId, BuildId) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(pool).await;
    let repository = PgIndustryRepository::new(pool.clone());
    let observation_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO blueprint_observations (
             id, workspace_id, owner_id, connection_id, eve_item_id, blueprint_type_id,
             captured_blueprint_name, blueprint_kind, material_efficiency, time_efficiency,
             licensed_runs, location_id, location_flag, captured_location_name, observed_at,
             source_payload, source_checksum
           ) VALUES ($1,$2,$3,NULL,$4,6830,'Rifter Blueprint','copy',$5,20,300,60003760,'Hangar',NULL,$6,'{}','checksum')"#,
    )
    .bind(observation_id)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(2_000_000_001_i64)
    .bind(observed_me)
    .bind(Utc::now())
    .execute(pool)
    .await
    .unwrap();
    let build = repository
        .create_root_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();
    let loaded = repository.get_build(workspace_id, build.id).await.unwrap();
    repository
        .update_draft(
            workspace_id,
            build.id,
            DraftUpdate {
                expected_revision: loaded.revision,
                name: loaded.name.clone(),
                runs: loaded.runs,
                notes: loaded.notes.clone(),
                replacement_recipe: None,
                draft_planning: Some(draft_planning_with_blueprint(
                    BlueprintSelection::ObservedAsset {
                        observation_id,
                        kind: BlueprintKind::Copy,
                        material_efficiency: 10,
                        time_efficiency: 20,
                        licensed_runs: None,
                    },
                )),
            },
        )
        .await
        .unwrap();
    (workspace_id, build.id)
}

async fn backfilled_licensed_runs(
    pool: &PgPool,
    workspace_id: iskworks_core::WorkspaceId,
    build_id: BuildId,
) -> Option<u64> {
    let build = PgIndustryRepository::new(pool.clone())
        .get_build(workspace_id, build_id)
        .await
        .unwrap();
    match build
        .draft_planning
        .unwrap()
        .input
        .blueprint_selection
        .unwrap()
    {
        BlueprintSelection::ObservedAsset { licensed_runs, .. } => licensed_runs,
        other => panic!("expected an ObservedAsset selection, got {other:?}"),
    }
}

// Multi-BPC job split: a copy captured before licensed runs were frozen
// gets them from its still-matching observation, idempotently.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn backfill_fills_a_captured_copys_licensed_runs(pool: PgPool) {
    let (workspace_id, build_id) = captured_copy_without_licensed_runs(&pool, 10).await;
    let service = IndustryService::new(
        Arc::new(PgIndustryRepository::new(pool.clone())),
        Arc::new(crate::PgSdeRepository::new(pool.clone())),
    );

    let first_run = service
        .backfill_observed_blueprint_configurations(workspace_id)
        .await
        .unwrap();
    assert_eq!(first_run.selections_captured, 1);
    assert!(first_run.selections_unresolved.is_empty());
    assert_eq!(
        backfilled_licensed_runs(&pool, workspace_id, build_id).await,
        Some(300)
    );

    let second_run = service
        .backfill_observed_blueprint_configurations(workspace_id)
        .await
        .unwrap();
    assert_eq!(second_run.builds_updated, 0);
}

// The observed copy was re-researched since capture (ME9 now): its runs no
// longer describe the frozen ME10 configuration, so they are not borrowed.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn backfill_leaves_licensed_runs_unset_when_the_copy_changed(pool: PgPool) {
    let (workspace_id, build_id) = captured_copy_without_licensed_runs(&pool, 9).await;
    let service = IndustryService::new(
        Arc::new(PgIndustryRepository::new(pool.clone())),
        Arc::new(crate::PgSdeRepository::new(pool.clone())),
    );

    let report = service
        .backfill_observed_blueprint_configurations(workspace_id)
        .await
        .unwrap();

    assert_eq!(report.builds_updated, 0);
    assert_eq!(report.selections_unresolved.len(), 1);
    assert_eq!(
        backfilled_licensed_runs(&pool, workspace_id, build_id).await,
        None
    );
}

// The same legacy shape, but the observation it names no longer exists
// (sold/moved -- the normal EVE event the backfill must handle). The
// backfill must leave the sentinel exactly as it was: never fabricated,
// never switched to Buy/Manual, reported as unresolved for a human to
// triage.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn backfill_leaves_an_unresolvable_legacy_selection_untouched(pool: PgPool) {
    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let repository = PgIndustryRepository::new(pool.clone());

    let missing_observation_id = Uuid::new_v4();
    let build = repository
        .create_root_build(NewBuild {
            build: draft_build(workspace_id, owner_id, manufacturing_recipe(import_id)),
        })
        .await
        .unwrap();
    let loaded = repository.get_build(workspace_id, build.id).await.unwrap();

    let legacy = draft_planning_with_blueprint(BlueprintSelection::ObservedAsset {
        observation_id: missing_observation_id,
        kind: BlueprintKind::Unknown,
        material_efficiency: 0,
        time_efficiency: 0,
        licensed_runs: None,
    });
    repository
        .update_draft(
            workspace_id,
            build.id,
            DraftUpdate {
                expected_revision: loaded.revision,
                name: loaded.name.clone(),
                runs: loaded.runs,
                notes: loaded.notes.clone(),
                replacement_recipe: None,
                draft_planning: Some(legacy),
            },
        )
        .await
        .unwrap();

    let service = IndustryService::new(
        Arc::new(PgIndustryRepository::new(pool.clone())),
        Arc::new(crate::PgSdeRepository::new(pool.clone())),
    );
    let report = service
        .backfill_observed_blueprint_configurations(workspace_id)
        .await
        .unwrap();

    assert_eq!(report.builds_updated, 0, "nothing resolvable to persist");
    assert_eq!(report.selections_unresolved.len(), 1);
    assert_eq!(report.selections_unresolved[0].build_id, build.id);
    assert_eq!(
        report.selections_unresolved[0].observation_id,
        missing_observation_id
    );

    // Never fabricated, never deleted, never switched to Buy/Manual.
    let unchanged = repository.get_build(workspace_id, build.id).await.unwrap();
    match unchanged
        .draft_planning
        .unwrap()
        .input
        .blueprint_selection
        .unwrap()
    {
        BlueprintSelection::ObservedAsset {
            kind,
            material_efficiency,
            time_efficiency,
            ..
        } => {
            assert_eq!(kind, BlueprintKind::Unknown);
            assert_eq!(material_efficiency, 0);
            assert_eq!(time_efficiency, 0);
        }
        other => panic!("expected the untouched sentinel, got {other:?}"),
    }
}

fn draft_planning_with_blueprint(
    blueprint_selection: iskworks_core::BlueprintSelection,
) -> DraftPlanningSnapshot {
    DraftPlanningSnapshot {
        input: DraftPlanningInput {
            material_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            output_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            manual_price_list_id: None,
            expected_manual_price_list_revision: None,
            material_pricing_policy: MarketPricingPolicy::HighestBuy,
            output_pricing_policy: MarketPricingPolicy::LowestSell,
            pricing_selections: Vec::new(),
            blueprint_selection: Some(blueprint_selection),
            manufacturing_facility: None,
            reaction_facility: None,
            facility_eiv_manual: false,
            component_resolutions: Vec::new(),
            fulfillment_scopes: Vec::new(),
        },
        updated_at: Utc::now(),
    }
}

/// The legacy-planner drop migration refuses to run while any root plan is
/// still on the legacy planner, and changes nothing when it refuses.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = false)]
async fn dropping_the_legacy_planner_fails_loudly_on_a_legacy_root(pool: PgPool) {
    const DROP_VERSION: i64 = 202_610_060_001;
    let mut migrator = sqlx::migrate!("../../migrations");
    migrator.migrations = std::borrow::Cow::Owned(
        migrator
            .migrations
            .iter()
            .filter(|migration| migration.version < DROP_VERSION)
            .cloned()
            .collect(),
    );
    migrator.run(&pool).await.unwrap();

    let (workspace_id, owner_id, import_id) = fixture_workspace(&pool).await;
    let legacy_root = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO builds (
          id, workspace_id, owner_id, display_name, recipe_kind,
          blueprint_type_id, blueprint_name, product_type_id, product_name,
          product_quantity_per_run, source_sde_dataset_id, source_sde_version,
          recipe_fingerprint, runs, notes, revision, created_at, updated_at,
          plan_root_build_id, planner_authority
        )
        VALUES ($1, $2, $3, 'Legacy root', 'manufacturing', 6830, 'Rifter Blueprint',
                587, 'Rifter', 1, $4, 'test', 'fp', 1, '', 1, now(), now(), $1, 'legacy')
        "#,
    )
    .bind(legacy_root)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(import_id)
    .execute(&pool)
    .await
    .unwrap();

    let refused = sqlx::raw_sql(include_str!(
        "../../../../migrations/202610060001_drop_legacy_planner.sql"
    ))
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("a root plan is still on the legacy planner"),
        "{refused}"
    );
    let authority: String =
        sqlx::query_scalar("SELECT planner_authority FROM builds WHERE id = $1")
            .bind(legacy_root)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(authority, "legacy", "nothing was dropped");
}
