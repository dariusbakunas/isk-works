use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use iskworks_api::{build_router, AppState};
use iskworks_core::fitting::{
    DamageProfile, FitCalculation, FitInput, FittingEngine, FittingEngineInfo, FittingError, Pilot,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ImportCounts, ManufacturingRecipe, SdeError,
    SdeReadRepository, TypeSearchResult,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

struct FakeSde;

#[async_trait]
impl SdeReadRepository for FakeSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(Some(ActiveSde {
            import_id: Uuid::new_v4(),
            source_version: "3482594".to_string(),
            source_label: "eve-online-static-data-3482594-jsonl.zip".to_string(),
            source_checksum: "abc".to_string(),
            completed_at: Utc::now(),
            counts: ImportCounts::default(),
        }))
    }
    async fn search_manufacturing_blueprints(
        &self,
        _: &str,
        _: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        unimplemented!()
    }
    async fn manufacturing_recipe(&self, _: i64) -> Result<Option<ManufacturingRecipe>, SdeError> {
        unimplemented!()
    }
    async fn search_types(&self, _: &str, _: u32) -> Result<Vec<TypeSearchResult>, SdeError> {
        unimplemented!()
    }
}

struct FakeEngine;

impl FittingEngine for FakeEngine {
    fn info(&self) -> FittingEngineInfo {
        FittingEngineInfo {
            data_version: "13.3650286.0".to_string(),
            major: 13,
            build_number: 3_650_286,
        }
    }

    fn calculate(
        &self,
        _: &FitInput,
        _: &Pilot,
        _: &DamageProfile,
    ) -> Result<FitCalculation, FittingError> {
        unimplemented!("meta does not calculate")
    }
}

fn state() -> AppState {
    AppState::new(Arc::new(configured_workspace("Fittings Test")))
        .with_sde_repository(Arc::new(FakeSde))
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn meta_reports_both_data_builds_when_the_engine_is_loaded() {
    let app = build_router(state().with_fitting_engine(Arc::new(FakeEngine)));
    let (status, body) = get(app, "/api/fittings/meta").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], true);
    assert_eq!(
        body["engine"],
        json!({ "dataVersion": "13.3650286.0", "major": 13, "buildNumber": 3_650_286 })
    );
    assert_eq!(body["sde"], json!({ "buildNumber": "3482594" }));
}

#[tokio::test]
async fn meta_says_unavailable_without_the_engine() {
    let (status, body) = get(build_router(state()), "/api/fittings/meta").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], false);
    assert_eq!(body["engine"], Value::Null);
    assert_eq!(body["sde"], json!({ "buildNumber": "3482594" }));
}

#[tokio::test]
async fn meta_lists_the_damage_profiles_uniform_first() {
    let app = build_router(state().with_fitting_engine(Arc::new(FakeEngine)));
    let (_, body) = get(app, "/api/fittings/meta").await;
    let profiles = body["damageProfiles"].as_array().unwrap();
    let ids: Vec<&str> = profiles
        .iter()
        .map(|profile| profile["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "uniform",
            "em",
            "thermal",
            "kinetic",
            "explosive",
            "angel-cartel",
            "blood-raiders",
            "guristas",
            "sanshas-nation",
            "serpentis"
        ]
    );
    assert_eq!(
        profiles[0],
        json!({ "id": "uniform", "name": "Uniform", "em": 25.0, "thermal": 25.0,
                "kinetic": 25.0, "explosive": 25.0 })
    );
    assert_eq!(
        profiles[7],
        json!({ "id": "guristas", "name": "Guristas", "em": 0.0, "thermal": 1828.0,
                "kinetic": 7413.0, "explosive": 0.0 })
    );
}
