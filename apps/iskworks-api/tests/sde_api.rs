use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use iskworks_api::{build_router, AppState};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ImportCounts, ManufacturingRecipe, ReactionFormulaRecipe,
    ReactionFormulaSearchResult, RecipeLine, SdeError, SdeReadRepository,
};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

mod support;

use support::workspace::EmptyWorkspaceRepository;

struct FixtureSdeRepository;

#[async_trait]
impl SdeReadRepository for FixtureSdeRepository {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(ActiveSde {
            import_id: Uuid::nil(),
            source_version: "123456".to_string(),
            source_label: "fixture.zip".to_string(),
            source_checksum: "abc123".to_string(),
            completed_at: Utc::now(),
            counts: ImportCounts {
                types: 4,
                blueprints: 1,
                material_lines: 1,
                product_lines: 1,
                skipped_blueprints: 0,
                ..Default::default()
            },
        }))
    }

    async fn search_manufacturing_blueprints(
        &self,
        query: &str,
        _limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        Ok(if query.to_lowercase().contains("rift") {
            vec![BlueprintSearchResult {
                blueprint_type_id: 6_830,
                blueprint_name: "Rifter Blueprint".to_string(),
                product_type_id: 5_876,
                product_name: "Rifter".to_string(),
                group_name: Some("Frigate".to_string()),
                published: true,
                manufacturing_available: true,
            }]
        } else {
            Vec::new()
        })
    }

    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        Ok((blueprint_type_id == 6_830).then(|| ManufacturingRecipe {
            blueprint_type_id,
            blueprint_name: "Rifter Blueprint".to_string(),
            duration_seconds: Some(600),
            materials: vec![RecipeLine {
                type_id: 34,
                type_name: "Tritanium".to_string(),
                quantity: 1_000,
            }],
            products: vec![RecipeLine {
                type_id: 5_876,
                type_name: "Rifter".to_string(),
                quantity: 1,
            }],
        }))
    }

    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<iskworks_sde::TypeSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn search_reaction_formulas(
        &self,
        query: &str,
        _limit: u32,
    ) -> Result<Vec<ReactionFormulaSearchResult>, SdeError> {
        Ok(if query.to_lowercase().contains("methano") {
            vec![ReactionFormulaSearchResult {
                reaction_formula_type_id: 46_157,
                reaction_formula_name: "Methanofullerene Reaction Formula".to_string(),
                product_type_id: 16_662,
                product_name: "Methanofullerene".to_string(),
                group_name: Some("Composite Reaction Formulas".to_string()),
                published: true,
            }]
        } else {
            Vec::new()
        })
    }

    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        Ok(
            (reaction_formula_type_id == 46_157).then(|| ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Methanofullerene Reaction Formula".to_string(),
                duration_seconds: Some(1_200),
                materials: vec![RecipeLine {
                    type_id: 16_272,
                    type_name: "Amber".to_string(),
                    quantity: 2,
                }],
                products: vec![RecipeLine {
                    type_id: 16_662,
                    type_name: "Methanofullerene".to_string(),
                    quantity: 100,
                }],
            }),
        )
    }

    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        Ok((product_type_id == 5_876).then_some(6_830))
    }

    async fn reaction_formula_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        Ok((product_type_id == 16_662).then_some(46_157))
    }
}

#[tokio::test]
async fn reports_active_sde_without_local_path() {
    let response = test_app().oneshot(get("/api/sde")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["configured"], true);
    assert_eq!(body["active"]["sourceVersion"], "123456");
    assert_eq!(body["active"]["sourceLabel"], "fixture.zip");
    assert!(body.to_string().find('/').is_none());
}

#[tokio::test]
async fn searches_manufacturing_products_and_blueprints() {
    let response = test_app()
        .oneshot(get("/api/blueprints/search?q=rift"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body[0]["blueprintTypeId"], 6_830);
    assert_eq!(body[0]["productName"], "Rifter");
    assert_eq!(body[0]["manufacturingAvailable"], true);
}

#[tokio::test]
async fn plans_direct_materials_for_requested_runs() {
    let response = test_app()
        .oneshot(get("/api/blueprints/6830/plan?runs=3"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["durationSeconds"], 1_800);
    assert_eq!(body["materials"][0]["totalQuantity"], 3_000);
    assert_eq!(body["products"][0]["totalQuantity"], 3);
}

#[tokio::test]
async fn rejects_invalid_runs_and_unknown_blueprints() {
    let invalid = test_app()
        .oneshot(get("/api/blueprints/6830/plan?runs=0"))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(invalid).await["error"]["code"],
        "invalid_runs"
    );

    let missing = test_app()
        .oneshot(get("/api/blueprints/42/plan?runs=1"))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(missing).await["error"]["code"],
        "blueprint_not_found"
    );

    let invalid_eiv = test_app()
        .oneshot(get("/api/blueprints/6830/estimated-item-value?runs=0"))
        .await
        .unwrap();
    assert_eq!(invalid_eiv.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(invalid_eiv).await["error"]["code"],
        "invalid_runs"
    );

    let missing_eiv = test_app()
        .oneshot(get("/api/blueprints/42/estimated-item-value?runs=1"))
        .await
        .unwrap();
    assert_eq!(missing_eiv.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(missing_eiv).await["error"]["code"],
        "blueprint_not_found"
    );
}

#[tokio::test]
async fn searches_reaction_products_and_formulas() {
    let response = test_app()
        .oneshot(get("/api/reaction-formulas/search?q=methano"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body[0]["reactionFormulaTypeId"], 46_157);
    assert_eq!(body[0]["productName"], "Methanofullerene");
}

#[tokio::test]
async fn plans_direct_reaction_materials_for_requested_runs() {
    let response = test_app()
        .oneshot(get("/api/reaction-formulas/46157/plan?runs=3"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["durationSeconds"], 3_600);
    assert_eq!(body["materials"][0]["totalQuantity"], 6);
    assert_eq!(body["products"][0]["totalQuantity"], 300);
}

#[tokio::test]
async fn rejects_invalid_runs_and_unknown_reaction_formulas() {
    let invalid = test_app()
        .oneshot(get("/api/reaction-formulas/46157/plan?runs=0"))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(invalid).await["error"]["code"],
        "invalid_runs"
    );

    let missing = test_app()
        .oneshot(get("/api/reaction-formulas/42/plan?runs=1"))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(missing).await["error"]["code"],
        "reaction_formula_not_found"
    );
}

#[tokio::test]
async fn rejects_invalid_runs_and_unknown_reaction_formulas_for_estimated_item_value() {
    let invalid_eiv = test_app()
        .oneshot(get(
            "/api/reaction-formulas/46157/estimated-item-value?runs=0",
        ))
        .await
        .unwrap();
    assert_eq!(invalid_eiv.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(invalid_eiv).await["error"]["code"],
        "invalid_runs"
    );

    let missing_eiv = test_app()
        .oneshot(get("/api/reaction-formulas/42/estimated-item-value?runs=1"))
        .await
        .unwrap();
    assert_eq!(missing_eiv.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(missing_eiv).await["error"]["code"],
        "reaction_formula_not_found"
    );
}

#[tokio::test]
async fn resolves_the_recipe_that_produces_a_given_product() {
    let manufacturing = test_app()
        .oneshot(get("/api/recipes/for-product/5876"))
        .await
        .unwrap();
    assert_eq!(manufacturing.status(), StatusCode::OK);
    let body = response_json(manufacturing).await;
    assert_eq!(body["mode"], "manufacturing");
    assert_eq!(body["blueprintTypeId"], 6_830);

    let reaction = test_app()
        .oneshot(get("/api/recipes/for-product/16662"))
        .await
        .unwrap();
    assert_eq!(reaction.status(), StatusCode::OK);
    let body = response_json(reaction).await;
    assert_eq!(body["mode"], "reaction");
    assert_eq!(body["reactionFormulaTypeId"], 46_157);

    let raw_material = test_app()
        .oneshot(get("/api/recipes/for-product/34"))
        .await
        .unwrap();
    assert_eq!(raw_material.status(), StatusCode::OK);
    assert_eq!(response_json(raw_material).await, Value::Null);
}

#[tokio::test]
async fn component_expansion_preview_resolves_buy_only_baseline() {
    let response = test_app()
        .oneshot(post(
            "/api/build-plans/component-expansion-preview",
            r#"{"root":{"mode":"manufacturing","blueprintTypeId":6830},"runs":1,"resolutions":[]}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let components = body["components"].as_array().unwrap();
    assert_eq!(components.len(), 1);
    assert_eq!(components[0]["typeId"], 34);
    assert_eq!(components[0]["totalQuantity"], 1_000);
    assert_eq!(components[0]["resolution"]["mode"], "buy");
}

#[tokio::test]
async fn component_expansion_preview_rejects_invalid_runs() {
    let response = test_app()
        .oneshot(post(
            "/api/build-plans/component-expansion-preview",
            r#"{"root":{"mode":"manufacturing","blueprintTypeId":6830},"runs":0,"resolutions":[]}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "invalid_runs");
}

#[tokio::test]
async fn component_expansion_preview_rejects_an_unresolvable_recipe_selection() {
    let response = test_app()
        .oneshot(post(
            "/api/build-plans/component-expansion-preview",
            r#"{
                "root": {"mode":"manufacturing","blueprintTypeId":6830},
                "runs": 1,
                "resolutions": [
                    {"typeId": 34, "recipe": {"mode":"manufacturing","blueprintTypeId":99999}}
                ]
            }"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "component_recipe_not_found");
}

fn test_app() -> axum::Router {
    build_router(
        AppState::new(Arc::new(EmptyWorkspaceRepository))
            .with_sde_repository(Arc::new(FixtureSdeRepository)),
    )
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn post(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}
