use super::*;

struct ConfigSde;

fn ln(type_id: i64, name: &str, quantity: i64) -> RecipeLine {
    RecipeLine {
        type_id,
        type_name: name.to_string(),
        quantity,
    }
}

#[async_trait]
impl SdeReadRepository for ConfigSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(ActiveSde {
            import_id: uuid::Uuid::nil(),
            source_version: "test".to_string(),
            source_label: "fixture.zip".to_string(),
            source_checksum: "abc123".to_string(),
            completed_at: chrono::Utc::now(),
            counts: iskworks_sde::ImportCounts {
                types: 10,
                blueprints: 4,
                material_lines: 4,
                product_lines: 4,
                skipped_blueprints: 0,
                ..Default::default()
            },
        }))
    }
    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        Ok(Vec::new())
    }
    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        Ok(Vec::new())
    }
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        let recipe = match blueprint_type_id {
            99_000 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Config Root Blueprint".to_string(),
                duration_seconds: Some(600),
                materials: vec![
                    ln(99_010, "Consumer A Product", 1),
                    ln(99_020, "Consumer B Product", 1),
                ],
                products: vec![ln(99_001, "Config Root Product", 1)],
            },
            99_011 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Config Consumer A Blueprint".to_string(),
                duration_seconds: Some(300),
                materials: vec![ln(99_100, "Config X", 24)],
                products: vec![ln(99_010, "Consumer A Product", 1)],
            },
            99_021 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Config Consumer B Blueprint".to_string(),
                duration_seconds: Some(300),
                materials: vec![ln(99_100, "Config X", 41)],
                products: vec![ln(99_020, "Consumer B Product", 1)],
            },
            // 2/run (not 1) so a 10% ME reduction produces a genuinely
            // different ceiling result (14 -> 13), not a same-either-way
            // number a facility-identity-only bug could still pass.
            99_101 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Config X Blueprint".to_string(),
                duration_seconds: Some(300),
                materials: vec![ln(34, "Tritanium", 2)],
                products: vec![ln(99_100, "Config X", 10)],
            },
            _ => return Ok(None),
        };
        Ok(Some(recipe))
    }
    async fn reaction_formula(
        &self,
        _reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        Ok(None)
    }
}

fn config_root(workspace_id: iskworks_core::WorkspaceId, owner_id: OwnerId) -> Build {
    Build {
        id: BuildId::new(),
        workspace_id,
        owner_id,
        name: "Config Root".to_string(),
        recipe: iskworks_core::BuildRecipe::Manufacturing(captured_recipe(
            99_000,
            &[
                (99_010, "Consumer A Product", 1),
                (99_020, "Consumer B Product", 1),
            ],
            (99_001, "Config Root Product", 1),
        )),
        runs: 1,
        notes: String::new(),
        revision: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        draft_planning: None,
        recipe_currency: iskworks_core::RecipeCurrency::Current,
        active_sde_version: Some("test".to_string()),
        product_category_name: None,
        product_group_name: None,
        selected_blueprint_origin: None,
        has_owned_blueprint: false,
    }
}

/// Root -> {ConsumerA -> X(24), ConsumerB -> X(41)}, X's own facility
/// pre-set to `facility_id` on both occurrences (so they start
/// compatible/poolable) -- the fixture behind every test in this
/// module. `only_consumer_a: true` omits ConsumerB/X_b entirely, giving
/// a single, non-shared descendant operation instead.
fn config_tree(
    workspace_id: iskworks_core::WorkspaceId,
    owner_id: OwnerId,
    root_id: BuildId,
    facility_id: iskworks_core::FacilityProfileId,
    only_consumer_a: bool,
) -> (Build, Option<Build>, Build, Option<Build>) {
    let consumer_a = linked_child_with_draft(
        workspace_id,
        owner_id,
        root_id,
        99_010,
        1,
        captured_recipe(
            99_011,
            &[(99_100, "Config X", 24)],
            (99_010, "Consumer A Product", 1),
        ),
        vec![resolution(99_100, 99_101)],
    );
    let mut x_a = linked_child_with_draft(
        workspace_id,
        owner_id,
        consumer_a.id,
        99_100,
        1,
        captured_recipe(99_101, &[(34, "Tritanium", 1)], (99_100, "Config X", 10)),
        Vec::new(),
    );
    x_a.draft_planning
        .as_mut()
        .unwrap()
        .input
        .manufacturing_facility = Some(iskworks_core::FacilityPreviewCommand {
        facility_profile_id: facility_id,
        blueprint_me: 0,
        blueprint_te: 0,
        estimated_item_value: None,
    });

    if only_consumer_a {
        return (consumer_a, None, x_a, None);
    }

    let consumer_b = linked_child_with_draft(
        workspace_id,
        owner_id,
        root_id,
        99_020,
        1,
        captured_recipe(
            99_021,
            &[(99_100, "Config X", 41)],
            (99_020, "Consumer B Product", 1),
        ),
        vec![resolution(99_100, 99_101)],
    );
    let mut x_b = linked_child_with_draft(
        workspace_id,
        owner_id,
        consumer_b.id,
        99_100,
        1,
        captured_recipe(99_101, &[(34, "Tritanium", 1)], (99_100, "Config X", 10)),
        Vec::new(),
    );
    x_b.draft_planning
        .as_mut()
        .unwrap()
        .input
        .manufacturing_facility = Some(iskworks_core::FacilityPreviewCommand {
        facility_profile_id: facility_id,
        blueprint_me: 0,
        blueprint_te: 0,
        estimated_item_value: None,
    });
    (consumer_a, Some(consumer_b), x_a, Some(x_b))
}

fn app_with(
    parent: Build,
    owner_id: OwnerId,
    linked_builds: Vec<Build>,
    facility_profiles: std::collections::HashMap<
        iskworks_core::FacilityProfileId,
        iskworks_core::IndustryFacilityProfile,
    >,
) -> axum::Router {
    let inventory = Arc::new(support::inventory::SeededInventoryRepository::new(
        Vec::new(),
    ));
    build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            linked_builds: std::sync::Mutex::new(linked_builds),
            facility_profiles,
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(ConfigSde))
        .with_inventory_repository(inventory)
        .with_production_repository(Arc::new(NeverCalledProductionRepository)),
    )
}

fn root_command(component_resolutions: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "recipe": {"mode": "manufacturing", "blueprintTypeId": 99_000},
        "runs": 1,
        "pricingSelections": [],
        "componentResolutions": component_resolutions,
        "fulfillmentScopes": [],
    })
}

fn one_consumer_resolution() -> serde_json::Value {
    serde_json::json!([
        {"typeId": 99_010, "recipe": {"mode": "manufacturing", "blueprintTypeId": 99_011}},
    ])
}

fn facility_patch_body(
    command: serde_json::Value,
    members: Vec<(BuildId, u64)>,
    facility_profile_id: Option<iskworks_core::FacilityProfileId>,
) -> serde_json::Value {
    serde_json::json!({
        "command": command,
        "members": members.into_iter().map(|(build_id, expected_revision)| {
            serde_json::json!({"buildId": build_id.0, "expectedRevision": expected_revision})
        }).collect::<Vec<_>>(),
        "kind": "facility",
        "facilityProfileId": facility_profile_id.map(|id| id.0),
    })
}

async fn patch_descendant_configuration(
    app: &axum::Router,
    root_id: BuildId,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!(
                    "/api/builds/{}/descendant-production-configuration",
                    root_id.0
                ))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    (status, json)
}

// REQUIRED: non-shared facility edit.
#[tokio::test]
async fn non_shared_facility_edit_patches_only_that_member_and_replans() {
    let workspace_id = iskworks_core::WorkspaceId::new();
    let owner_id = OwnerId::new();
    let root = config_root(workspace_id, owner_id);
    let root_id = root.id;
    let facility_a = iskworks_core::FacilityProfileId::new();
    let facility_b = iskworks_core::FacilityProfileId::new();
    let (consumer_a, _, x_a, _) = config_tree(workspace_id, owner_id, root_id, facility_a, true);
    let x_a_id = x_a.id;

    let facility_profiles = std::collections::HashMap::from([
        (facility_a, manufacturing_facility_at(facility_a, 1, 0)),
        (facility_b, manufacturing_facility_at(facility_b, 1, 10)),
    ]);
    let app = app_with(root, owner_id, vec![consumer_a, x_a], facility_profiles);

    let (status, json) = patch_descendant_configuration(
        &app,
        root_id,
        facility_patch_body(
            root_command(one_consumer_resolution()),
            vec![(x_a_id, 1)],
            Some(facility_b),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json:?}");
    let builds = json.as_array().expect("response is an array of Builds");
    assert_eq!(builds.len(), 1);
    assert_eq!(builds[0]["id"], x_a_id.0.to_string());
    assert_eq!(builds[0]["revision"], 2, "revision bumped exactly once");
    // The canonical linked Build's own persisted configuration now
    // names facility B directly -- not inferred from a side effect.
    assert_eq!(
        builds[0]["draftPlanning"]["input"]["manufacturingFacility"]["facilityProfileId"],
        facility_b.0.to_string()
    );

    // Replanning confirms it: X's own downstream Tritanium demand
    // reflects facility B's real preview. 3 runs * 2/run = 6 base;
    // facility B's 10% reduction gives ceil(6 * 0.9) = 6 too (rounds to
    // the same figure at this run count), so the identity check above
    // -- not this number -- is what actually proves the facility
    // changed; this just confirms the replanned tree is still
    // internally consistent.
    let (status, json) =
        post_materials(app, root_id, root_command(one_consumer_resolution())).await;
    assert_eq!(status, StatusCode::OK, "{json:?}");
    assert_eq!(row(&json, 34).unwrap()["requiredQuantity"], 6);
}

#[tokio::test]
async fn no_members_and_root_member_are_rejected_as_validation_errors() {
    let workspace_id = iskworks_core::WorkspaceId::new();
    let owner_id = OwnerId::new();
    let root = config_root(workspace_id, owner_id);
    let root_id = root.id;
    let facility_1 = iskworks_core::FacilityProfileId::new();
    let (consumer_a, _, x_a, _) = config_tree(workspace_id, owner_id, root_id, facility_1, true);
    let facility_profiles = std::collections::HashMap::from([(
        facility_1,
        manufacturing_facility_at(facility_1, 1, 0),
    )]);
    let app = app_with(root, owner_id, vec![consumer_a, x_a], facility_profiles);

    let (status, json) = patch_descendant_configuration(
        &app,
        root_id,
        facility_patch_body(root_command(one_consumer_resolution()), vec![], None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json:?}");
    assert_eq!(json["error"]["code"], "validation_failed");

    let (status, json) = patch_descendant_configuration(
        &app,
        root_id,
        facility_patch_body(
            root_command(one_consumer_resolution()),
            vec![(root_id, 1)],
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json:?}");
    assert_eq!(json["error"]["code"], "validation_failed");
}
