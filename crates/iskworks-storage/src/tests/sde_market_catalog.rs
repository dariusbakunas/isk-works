use super::*;

/// `list_market_items` is the item summary table's candidate list --
/// published, market-grouped types only, exact market-group match,
/// case-insensitive name search, paginated. Five minerals share market
/// group 1857; a sixth type in a different group and a seventh, unpublished
/// type in the *same* group as the minerals both prove exclusion.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_market_items_filters_by_category_and_search_and_paginates(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let dataset = market_items_fixture();
    let import_id = repository
        .begin_import(NewImport {
            source_version: dataset.source_version.clone(),
            source_label: dataset.source_label.clone(),
            source_checksum: dataset.source_checksum.clone(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    repository
        .write_dataset(import_id, &dataset, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(import_id, dataset.counts(), Utc::now())
        .await
        .unwrap();

    // Page 1 of 3: alphabetically first 3 of the 5 minerals, correct total.
    let (page_one, total) = repository
        .list_market_items(Some(1_857), "", 1, 3)
        .await
        .unwrap();
    assert_eq!(total, 5);
    assert_eq!(
        page_one
            .iter()
            .map(|item| item.type_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Isogen", "Mexallon", "Nocxium"]
    );

    // Page 2: the remaining 2 -- neither the wrong-group type (Rifter) nor
    // the unpublished same-group type (Unpublished Ore) ever appear.
    let (page_two, total) = repository
        .list_market_items(Some(1_857), "", 2, 3)
        .await
        .unwrap();
    assert_eq!(total, 5);
    assert_eq!(
        page_two
            .iter()
            .map(|item| item.type_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Pyerite", "Tritanium"]
    );
    assert!(!page_one
        .iter()
        .chain(&page_two)
        .any(|item| item.type_name == "Unpublished Ore" || item.type_name == "Rifter"));

    // Search alone (no category) finds Tritanium across the whole catalog.
    let (search_results, search_total) = repository
        .list_market_items(None, "trit", 1, 50)
        .await
        .unwrap();
    assert_eq!(search_total, 1);
    assert_eq!(search_results[0].type_name, "Tritanium");

    // A category with nothing published in it returns empty, not an error.
    let (empty, empty_total) = repository
        .list_market_items(Some(999_999), "", 1, 50)
        .await
        .unwrap();
    assert_eq!(empty_total, 0);
    assert!(empty.is_empty());
}

fn market_items_fixture() -> iskworks_sde::NormalizedSde {
    iskworks_sde::NormalizedSde {
        source_label: "market-items-fixture.zip".to_string(),
        source_checksum: "market-items-fixture-checksum".to_string(),
        source_version: "1".to_string(),
        types: vec![
            ImportType {
                type_id: 34,
                name: "Tritanium".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(1_857),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 35,
                name: "Pyerite".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(1_857),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 36,
                name: "Mexallon".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(1_857),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 37,
                name: "Isogen".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(1_857),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 40,
                name: "Nocxium".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(1_857),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            // Same market group, but unpublished -- must never appear.
            ImportType {
                type_id: 41,
                name: "Unpublished Ore".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(1_857),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: false,
            },
            // Published, but a different market group -- must never appear
            // under a `marketGroupId=1857` filter.
            ImportType {
                type_id: 5_876,
                name: "Rifter".to_string(),
                group_id: Some(25),
                group_name: Some("Frigate".to_string()),
                market_group_id: Some(1_361),
                meta_group_id: Some(1),
                packaged_volume_m3: None,
                published: true,
            },
        ],
        categories: Vec::new(),
        groups: Vec::new(),
        meta_groups: Vec::new(),
        market_groups: Vec::new(),
        blueprints: Vec::new(),
        solar_systems: Vec::new(),
        constellations: Vec::new(),
        regions: Vec::new(),
        npc_stations: Vec::new(),
        structure_modifiers: Vec::new(),
        rig_modifiers: Vec::new(),
        reaction_formulas: Vec::new(),
        reaction_rig_modifiers: Vec::new(),
        industry_target_filters: Vec::new(),
        planet_schematics: Vec::new(),
        planets: Vec::new(),
        skipped_blueprints: 0,
        skipped_reaction_formulas: 0,
    }
}

/// `list_market_group_subtree_item_ids` is descendant-inclusive (Root ->
/// Child -> Grandchild), matching `MarketCategoryNode.item_count`'s own
/// rollup exactly, while an unpublished item and a sibling subtree stay
/// excluded.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_market_group_subtree_item_ids_includes_every_descendant_exactly_once(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let dataset = market_group_subtree_fixture();
    let import_id = repository
        .begin_import(NewImport {
            source_version: dataset.source_version.clone(),
            source_label: dataset.source_label.clone(),
            source_checksum: dataset.source_checksum.clone(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    repository
        .write_dataset(import_id, &dataset, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(import_id, dataset.counts(), Utc::now())
        .await
        .unwrap();

    // Walking from the root includes the root's own item plus every
    // descendant's, exactly once each, and excludes the unpublished item
    // and the unrelated sibling subtree.
    let mut items = repository
        .list_market_group_subtree_item_ids(1)
        .await
        .unwrap();
    items.sort_by_key(|item| item.type_id);
    assert_eq!(
        items
            .iter()
            .map(|item| item.type_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Root Item", "Child Item", "Grandchild Item"]
    );

    // Walking from the leaf returns just that leaf's own item.
    let leaf_items = repository
        .list_market_group_subtree_item_ids(3)
        .await
        .unwrap();
    assert_eq!(leaf_items.len(), 1);
    assert_eq!(leaf_items[0].type_name, "Grandchild Item");

    // The rolled-up count matches exactly what the category tree already
    // shows for the same root -- the two must never disagree.
    let flat = repository.list_market_groups().await.unwrap();
    let tree = iskworks_core::build_market_category_tree(flat);
    let root_node = tree
        .iter()
        .find(|node| node.market_group_id == 1)
        .expect("root group present in tree");
    assert_eq!(root_node.item_count as usize, items.len());
}

fn market_group_subtree_fixture() -> iskworks_sde::NormalizedSde {
    iskworks_sde::NormalizedSde {
        source_label: "market-group-subtree-fixture.zip".to_string(),
        source_checksum: "market-group-subtree-fixture-checksum".to_string(),
        source_version: "1".to_string(),
        types: vec![
            ImportType {
                type_id: 100,
                name: "Root Item".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(1),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 101,
                name: "Child Item".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(2),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            ImportType {
                type_id: 102,
                name: "Grandchild Item".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(3),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
            // Same group as the grandchild item, but unpublished -- must
            // never appear.
            ImportType {
                type_id: 103,
                name: "Unpublished Grandchild Item".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(3),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: false,
            },
            // A published item in a completely unrelated root group -- must
            // never appear when walking group 1's subtree.
            ImportType {
                type_id: 104,
                name: "Unrelated Item".to_string(),
                group_id: Some(18),
                group_name: Some("Mineral".to_string()),
                market_group_id: Some(999),
                meta_group_id: None,
                packaged_volume_m3: None,
                published: true,
            },
        ],
        categories: Vec::new(),
        groups: Vec::new(),
        meta_groups: Vec::new(),
        market_groups: vec![
            ImportMarketGroup {
                market_group_id: 1,
                name: "Root".to_string(),
                parent_group_id: None,
            },
            ImportMarketGroup {
                market_group_id: 2,
                name: "Child".to_string(),
                parent_group_id: Some(1),
            },
            ImportMarketGroup {
                market_group_id: 3,
                name: "Grandchild".to_string(),
                parent_group_id: Some(2),
            },
            ImportMarketGroup {
                market_group_id: 999,
                name: "Unrelated".to_string(),
                parent_group_id: None,
            },
        ],
        blueprints: Vec::new(),
        solar_systems: Vec::new(),
        constellations: Vec::new(),
        regions: Vec::new(),
        npc_stations: Vec::new(),
        structure_modifiers: Vec::new(),
        rig_modifiers: Vec::new(),
        reaction_formulas: Vec::new(),
        reaction_rig_modifiers: Vec::new(),
        industry_target_filters: Vec::new(),
        planet_schematics: Vec::new(),
        planets: Vec::new(),
        skipped_blueprints: 0,
        skipped_reaction_formulas: 0,
    }
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn strict_scope_classification_against_real_sql(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    let dataset = strict_opportunity_fixture();
    let import_id = repository
        .begin_import(NewImport {
            source_version: dataset.source_version.clone(),
            source_label: dataset.source_label.clone(),
            source_checksum: dataset.source_checksum.clone(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    repository
        .write_dataset(import_id, &dataset, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(import_id, dataset.counts(), Utc::now())
        .await
        .unwrap();

    let frigate_scope =
        iskworks_core::profitability_scope(iskworks_core::ProfitabilityScopeId::T1Frigates)
            .unwrap();
    let frigates = repository
        .manufacturable_candidates(&frigate_scope.candidate_scope)
        .await
        .unwrap();
    assert_eq!(
        frigates
            .iter()
            .map(|candidate| candidate.primary_product_type_id)
            .collect::<Vec<_>>(),
        vec![77_114, 5_876]
    );
    let metamorphosis = frigates
        .iter()
        .find(|candidate| candidate.primary_product_type_id == 77_114)
        .unwrap();
    assert_eq!(
        metamorphosis.classification.market_group_ancestry,
        vec![
            iskworks_sde::CandidateMarketGroup {
                market_group_id: 4,
                name: "Ships".into(),
            },
            iskworks_sde::CandidateMarketGroup {
                market_group_id: 1_612,
                name: "Special Edition Ships".into(),
            },
            iskworks_sde::CandidateMarketGroup {
                market_group_id: 1_619,
                name: "Special Edition Frigates".into(),
            },
        ]
    );

    let battleship_scope =
        iskworks_core::profitability_scope(iskworks_core::ProfitabilityScopeId::T1Battleships)
            .unwrap();
    let battleships = repository
        .manufacturable_candidates(&battleship_scope.candidate_scope)
        .await
        .unwrap();
    assert_eq!(
        battleships
            .iter()
            .map(|candidate| candidate.primary_product_type_id)
            .collect::<Vec<_>>(),
        vec![24_605]
    );

    // Rigs: classified by category + market-group-root against real
    // recursive SQL, not the in-memory check alone. The Tech II rig shares
    // every other dimension (category, group, market group) with its Tech I
    // sibling -- only `meta_group_id` tells them apart.
    let rigs_scope =
        iskworks_core::profitability_scope(iskworks_core::ProfitabilityScopeId::T1Rigs).unwrap();
    let rigs = repository
        .manufacturable_candidates(&rigs_scope.candidate_scope)
        .await
        .unwrap();
    assert_eq!(
        rigs.iter()
            .map(|candidate| candidate.primary_product_type_id)
            .collect::<Vec<_>>(),
        vec![30_987]
    );

    // Reactions: the storage layer's manufacturing/reaction UNION actually
    // reaches this scope's `recipe_kinds: {Reaction}` filter, and a product
    // with no meta_group_id at all (real SDE behavior for reaction
    // materials) is still captured rather than silently dropped.
    let reactions_scope =
        iskworks_core::profitability_scope(iskworks_core::ProfitabilityScopeId::Reactions).unwrap();
    let reactions = repository
        .manufacturable_candidates(&reactions_scope.candidate_scope)
        .await
        .unwrap();
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].primary_product_type_id, 16_656);
    assert!(matches!(
        reactions[0].identity,
        iskworks_sde::CandidateRecipeIdentity::Reaction {
            reaction_formula_type_id: 46_171
        }
    ));
    assert_eq!(reactions[0].classification.meta_group_id, None);

    let scanner_table_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_name IN ('opportunity', 'profitability_candidate', 'scan_run')
        "#,
    )
    .fetch_one(repository.pool())
    .await
    .unwrap();
    assert_eq!(scanner_table_count, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn opportunity_evaluation_registers_deduplicated_local_evidence_without_domain_writes(
    pool: PgPool,
) {
    use iskworks_core::{
        CreateFacilityProfileCommand, EvaluateOpportunitiesCommand, FacilityKind, FacilityRole,
        MarketRepository, OpportunityQueryService, ProfitabilityScopeId, SecurityClass,
    };
    use std::sync::Arc;

    let created = WorkspaceService::new(PgWorkspaceRepository::new(pool.clone()))
        .create_workspace(CreateWorkspaceCommand {
            name: "Opportunity Integration".to_string(),
        })
        .await
        .unwrap();
    let workspace_id = created.workspace.unwrap().id;
    let owner_id = created.owner.unwrap().id;
    let sde = Arc::new(PgSdeRepository::new(pool.clone()));
    let dataset = strict_opportunity_fixture();
    let import_id = sde
        .begin_import(NewImport {
            source_version: dataset.source_version.clone(),
            source_label: dataset.source_label.clone(),
            source_checksum: dataset.source_checksum.clone(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    sde.write_dataset(import_id, &dataset, &NoopProgressReporter)
        .await
        .unwrap();
    sde.activate_import(import_id, dataset.counts(), Utc::now())
        .await
        .unwrap();

    let profile = iskworks_core::parse_profile(
        workspace_id,
        CreateFacilityProfileCommand {
            name: "Manual manufacturing".to_string(),
            kind: FacilityKind::NpcStation,
            role: FacilityRole::Manufacturing,
            structure_id: None,
            structure_type_id: None,
            structure_type_name: String::new(),
            solar_system_id: Some(30_000_142),
            solar_system_name: "Jita".to_string(),
            security_class: SecurityClass::HighSec,
            material_reduction_percent: "0".to_string(),
            time_reduction_percent: "0".to_string(),
            job_cost_reduction_percent: "0".to_string(),
            facility_tax_percent: "0".to_string(),
            scc_surcharge_percent: "0".to_string(),
            alliance_surcharge_percent: "0".to_string(),
            fixed_supplemental_cost: "0".to_string(),
            manual_system_cost_index: Some("0.05".to_string()),
            notes: String::new(),
            rigs: Vec::new(),
        },
    )
    .unwrap();
    let profile = PgFacilityRepository::new(pool.clone())
        .create(profile)
        .await
        .unwrap();
    let market = Arc::new(PgMarketRepository::new(pool.clone()));
    let source_id = market
        .ensure_esi_price_source_for_scope(workspace_id, iskworks_core::DEFAULT_MARKET_SCOPE)
        .await
        .unwrap();
    let service = OpportunityQueryService::new(
        Arc::new(PgIndustryRepository::new(pool.clone())),
        sde,
        market,
        Arc::new(PgEsiRepository::new(pool.clone())),
    );

    let evaluation = service
        .evaluate(
            workspace_id,
            owner_id,
            EvaluateOpportunitiesCommand {
                scope_id: ProfitabilityScopeId::T1Frigates,
                facility_profile_id: profile.id,
                material_efficiency: Some(10),
                time_efficiency: Some(20),
                market_scope: iskworks_core::DEFAULT_MARKET_SCOPE,
            },
            Utc::now(),
        )
        .await
        .unwrap();

    // The `strict_opportunity_fixture` `T1Frigates` set has two group-25
    // Tech I published hulls: Rifter (5876) and the special-edition
    // Metamorphosis (77114). Both are *discovered* candidates; Metamorphosis
    // is then excluded from the default ranking by the special-edition
    // market-group ancestry rule -- discovery still evaluates it and still
    // registers its market demand.
    assert_eq!(evaluation.candidate_count, 2);
    assert_eq!(evaluation.default_ranking_eligible_count, 1);
    assert_eq!(evaluation.excluded_count, 1);
    // Both blueprints consume only Tritanium (34): the shared material demand
    // is deduplicated to a single required type.
    assert_eq!(evaluation.readiness.required_material_type_count, 1);
    // One required output per discovered candidate: Rifter and Metamorphosis.
    assert_eq!(evaluation.readiness.required_output_type_count, 2);
    let registered: Vec<i64> = sqlx::query_scalar(
        "SELECT type_id FROM market_source_coverage WHERE workspace_id=$1 AND price_source_id=$2 ORDER BY type_id",
    ).bind(workspace_id.0).bind(source_id.0).fetch_all(&pool).await.unwrap();
    assert_eq!(registered, vec![34, 5_876, 77_114]);
    // The evaluation must not have written any domain rows -- only local
    // market-coverage evidence; `builds` is the manufacturing-domain table.
    let build_count: i64 = sqlx::query_scalar("SELECT count(*) FROM builds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(build_count, 0);
}
