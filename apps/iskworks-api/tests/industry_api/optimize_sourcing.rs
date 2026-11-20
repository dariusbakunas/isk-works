//! Optimize sourcing: preview and apply over a canonical plan.
//!
//! Root (manufacturing) needs 100 X; X is reacted from 100 Y; Y is reacted
//! from 100 Z (raw). Every Build sits in one facility per activity with a
//! 5 % system cost index and adjusted prices of 1, so each job costs 5 ISK
//! to install and every figure below is exact.

use super::*;
use iskworks_core::production_dependency::DependencySourcing;

const ROOT_BP: i64 = 96_000;
const ROOT_PRODUCT: i64 = 96_001;
const X: i64 = 96_100;
const X_FORMULA: i64 = 96_101;
const Y: i64 = 96_200;
const Y_FORMULA: i64 = 96_201;
const Z: i64 = 96_900;

struct ChainSde;

fn ln(type_id: i64, name: &str, quantity: i64) -> RecipeLine {
    RecipeLine {
        type_id,
        type_name: name.to_string(),
        quantity,
    }
}

fn formula(reaction_formula_type_id: i64) -> Option<ReactionFormulaRecipe> {
    let (materials, product) = match reaction_formula_type_id {
        X_FORMULA => (vec![ln(Y, "Y", 100)], ln(X, "X", 100)),
        Y_FORMULA => (vec![ln(Z, "Z", 100)], ln(Y, "Y", 100)),
        _ => return None,
    };
    Some(ReactionFormulaRecipe {
        reaction_formula_type_id,
        reaction_formula_name: format!("formula {reaction_formula_type_id}"),
        duration_seconds: Some(3_600),
        materials,
        products: vec![product],
    })
}

#[async_trait]
impl iskworks_sde::FakeSdeRead for ChainSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(ActiveSde {
            import_id: uuid::Uuid::nil(),
            source_version: "test".to_string(),
            source_label: "fixture.zip".to_string(),
            source_checksum: "abc123".to_string(),
            completed_at: chrono::Utc::now(),
            counts: iskworks_sde::ImportCounts::default(),
        }))
    }
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        Ok((blueprint_type_id == ROOT_BP).then(|| ManufacturingRecipe {
            blueprint_type_id,
            blueprint_name: "Root Blueprint".to_string(),
            duration_seconds: Some(300),
            materials: vec![ln(X, "X", 100)],
            products: vec![ln(ROOT_PRODUCT, "Root Product", 1)],
        }))
    }
    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        Ok((product_type_id == ROOT_PRODUCT).then_some(ROOT_BP))
    }
    async fn reaction_formula_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        Ok(match product_type_id {
            X => Some(X_FORMULA),
            Y => Some(Y_FORMULA),
            _ => None,
        })
    }
    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        Ok(formula(reaction_formula_type_id))
    }
    async fn production_recipes_for_products(
        &self,
        product_type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, iskworks_sde::ProductRecipeRefs>, SdeError> {
        Ok(product_type_ids
            .iter()
            .filter_map(|type_id| {
                let refs = match *type_id {
                    ROOT_PRODUCT => iskworks_sde::ProductRecipeRefs {
                        blueprint_type_id: Some(ROOT_BP),
                        reaction_formula_type_id: None,
                    },
                    X => iskworks_sde::ProductRecipeRefs {
                        blueprint_type_id: None,
                        reaction_formula_type_id: Some(X_FORMULA),
                    },
                    Y => iskworks_sde::ProductRecipeRefs {
                        blueprint_type_id: None,
                        reaction_formula_type_id: Some(Y_FORMULA),
                    },
                    _ => return None,
                };
                Some((*type_id, refs))
            })
            .collect())
    }
    async fn type_classifications(
        &self,
        _type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, iskworks_sde::SdeTypeClassification>, SdeError>
    {
        Ok(std::collections::BTreeMap::new())
    }
    async fn inventory_type_metadata(
        &self,
        _type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, iskworks_sde::SdeInventoryTypeMetadata>, SdeError>
    {
        Ok(std::collections::BTreeMap::new())
    }
    async fn type_reference(
        &self,
        _type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, iskworks_sde::TypeReference>, SdeError> {
        Ok(std::collections::BTreeMap::new())
    }
}

struct Fixture {
    router: axum::Router,
    repository: Arc<FixtureIndustryRepository>,
    root: BuildId,
}

/// The all-Buy plan, with market prices X = `x`, Y = `y`, Z = 1.
fn fixture(x: &str, y: &str) -> Fixture {
    let workspace_id = iskworks_core::WorkspaceId::new();
    let owner_id = OwnerId::new();
    let manufacturing = fixture_facility_profile(iskworks_core::FacilityRole::Manufacturing);
    let reactions = fixture_facility_profile(iskworks_core::FacilityRole::Reaction);

    let mut root = linked_child_with_draft(
        workspace_id,
        owner_id,
        BuildId::new(),
        0,
        1,
        captured_recipe(ROOT_BP, &[(X, "X", 100)], (ROOT_PRODUCT, "Root Product", 1)),
        Vec::new(),
    );
    fixture_unlink(root.id);
    root.name = "Root".to_string();
    let draft = &mut root.draft_planning.as_mut().unwrap().input;
    draft.manufacturing_facility = Some(iskworks_core::FacilityPreviewCommand {
        facility_profile_id: manufacturing.id,
        blueprint_me: 0,
        blueprint_te: 0,
        estimated_item_value: None,
    });
    draft.reaction_facility = Some(iskworks_core::ReactionFacilityPreviewCommand {
        facility_profile_id: reactions.id,
        estimated_item_value: None,
    });
    let root_id = root.id;

    let market_items = [
        (Z, price_item(Z, "Z", "1")),
        (Y, price_item(Y, "Y", y)),
        (X, price_item(X, "X", x)),
        (
            ROOT_PRODUCT,
            price_item(ROOT_PRODUCT, "Root Product", "10000"),
        ),
    ]
    .into_iter()
    .collect();
    let repository = Arc::new(FixtureIndustryRepository {
        build: Some(root),
        market_items,
        facility_profiles: [(manufacturing.id, manufacturing), (reactions.id, reactions)]
            .into_iter()
            .collect(),
        ..Default::default()
    });
    let mut new_workspace = NewWorkspace::manual("Industry".to_string());
    new_workspace.workspace.owner_id = owner_id;
    new_workspace.owner.id = owner_id;
    let router = build_router(
        AppState::new(Arc::new(ConfiguredWorkspaceRepository {
            state: WorkspaceState::configured(new_workspace.workspace, new_workspace.owner),
        }))
        .with_industry_repository(repository.clone())
        .with_sde_repository(Arc::new(ChainSde))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        .with_production_repository(Arc::new(NeverCalledProductionRepository))
        .with_adjusted_price_repository(Arc::new(FixtureAdjustedPriceRepository {
            prices: [X, Y, Z]
                .into_iter()
                .map(|type_id| (type_id, rust_decimal::Decimal::ONE))
                .collect(),
        })),
    );
    Fixture {
        router,
        repository,
        root: root_id,
    }
}

impl Fixture {
    async fn post(&self, uri: String, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    async fn preview_of(
        &self,
        build: BuildId,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        self.post(
            format!("/api/builds/{}/optimize-sourcing/preview", build.0),
            body,
        )
        .await
    }

    async fn preview(&self, body: serde_json::Value) -> serde_json::Value {
        let (status, json) = self.preview_of(self.root, body).await;
        assert_eq!(status, StatusCode::OK, "{json}");
        json
    }

    async fn apply(&self, proposal: &serde_json::Value) -> (StatusCode, serde_json::Value) {
        let changes: Vec<serde_json::Value> = proposal["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|change| serde_json::json!({ "typeId": change["typeId"], "sourcing": change["to"] }))
            .collect();
        self.post(
            format!("/api/builds/{}/optimize-sourcing/apply", self.root.0),
            serde_json::json!({ "fingerprint": proposal["fingerprint"], "changes": changes }),
        )
        .await
    }

    fn edge_producer(&self, consumer: BuildId, type_id: i64) -> Option<BuildId> {
        let records = self.repository.canonical_records().unwrap();
        let edge = records
            .dependencies
            .iter()
            .find(|edge| edge.consumer_build_id == consumer && edge.component_type_id == type_id)
            .unwrap_or_else(|| panic!("no {type_id} edge on {consumer:?}"));
        match edge.sourcing {
            DependencySourcing::Buy => None,
            DependencySourcing::Produce { .. } => edge.producer_build_id,
        }
    }
}

fn money(value: &serde_json::Value) -> rust_decimal::Decimal {
    value
        .as_str()
        .unwrap_or_else(|| panic!("not money: {value}"))
        .parse()
        .unwrap()
}

fn changes(proposal: &serde_json::Value) -> Vec<(i64, String)> {
    proposal["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|change| {
            (
                change["typeId"].as_i64().unwrap(),
                change["to"]["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn preview_proposes_reacting_a_cheap_chain_and_apply_writes_it_top_down() {
    let app = fixture("5", "3");

    // Current: buy 100 X at 5 + root install 5 = 505.
    // Produce: Y = 100 Z + 5 = 105 (1.05/u < 3); X = 105 + 5 = 110
    // (1.1/u < 5); root = 110 + 5 = 115.
    let proposal = app.preview(serde_json::json!({})).await;
    assert!(proposal["notice"].is_null(), "{proposal}");
    assert_eq!(
        money(&proposal["currentCost"]),
        "505".parse().unwrap(),
        "{proposal}"
    );
    assert_eq!(money(&proposal["proposedCost"]), "115".parse().unwrap());
    assert_eq!(money(&proposal["delta"]), "390".parse().unwrap());
    assert_eq!(
        money(&proposal["proposedMargin"]) - money(&proposal["currentMargin"]),
        "390".parse().unwrap()
    );
    assert_eq!(
        changes(&proposal),
        vec![(X, "produce".to_string()), (Y, "produce".to_string())]
    );
    assert_eq!(proposal["changes"][0]["from"], "buy");
    assert!(
        proposal["changes"][1]["from"].is_null(),
        "Y is only reached through X"
    );
    assert_eq!(proposal["changes"][1]["depth"], 2);
    // Read-only: nothing written yet.
    assert_eq!(app.edge_producer(app.root, X), None);

    let (status, outcome) = app.apply(&proposal).await;
    assert_eq!(status, StatusCode::OK, "{outcome}");
    assert_eq!(outcome["applied"], serde_json::json!([X, Y]));
    assert_eq!(outcome["failed"], serde_json::json!([]));

    let x_producer = app.edge_producer(app.root, X).expect("X is produced");
    let y_producer = app
        .edge_producer(x_producer, Y)
        .expect("the new X producer reacts Y");
    assert_eq!(app.edge_producer(y_producer, Z), None, "Z stays bought");

    // Idempotent: the applied plan is already optimal.
    let again = app.preview(serde_json::json!({})).await;
    assert_eq!(changes(&again), Vec::new());
    assert_eq!(money(&again["currentCost"]), "115".parse().unwrap());
    assert_eq!(money(&again["delta"]), rust_decimal::Decimal::ZERO);

    // Only the root plans sourcing.
    let (status, json) = app.preview_of(x_producer, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
}

#[tokio::test]
async fn a_cheaper_to_buy_intermediate_is_left_bought() {
    // X sells at 1: reacting it (1.1/u) is dearer -> nothing to switch.
    let app = fixture("1", "3");
    let proposal = app.preview(serde_json::json!({})).await;
    assert_eq!(changes(&proposal), Vec::new(), "{proposal}");
    assert_eq!(proposal["currentCost"], proposal["proposedCost"]);
}

#[tokio::test]
async fn excluded_components_keep_their_sourcing() {
    let app = fixture("5", "3");
    let proposal = app.preview(serde_json::json!({ "exclude": [X] })).await;
    assert_eq!(changes(&proposal), Vec::new(), "{proposal}");
}

#[tokio::test]
async fn the_threshold_suppresses_small_savings() {
    // X at 1.12 vs 1.1 to react: a ~1.8 % saving.
    let app = fixture("1.12", "3");
    let strict = app
        .preview(serde_json::json!({ "minSavingPercent": "2" }))
        .await;
    assert_eq!(changes(&strict), Vec::new(), "{strict}");
    let any = app
        .preview(serde_json::json!({ "minSavingPercent": "0" }))
        .await;
    assert_eq!(
        changes(&any),
        vec![(X, "produce".to_string()), (Y, "produce".to_string())]
    );
}

#[tokio::test]
async fn apply_refuses_a_plan_that_changed_since_the_preview() {
    let app = fixture("5", "3");
    let mut proposal = app.preview(serde_json::json!({})).await;
    proposal["fingerprint"] = serde_json::json!("stale");
    let (status, json) = app.apply(&proposal).await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    assert_eq!(app.edge_producer(app.root, X), None, "nothing written");
}

#[tokio::test]
async fn an_out_of_range_threshold_is_rejected() {
    let app = fixture("5", "3");
    let (status, _) = app
        .preview_of(app.root, serde_json::json!({ "minSavingPercent": "100" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
