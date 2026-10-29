use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_core::fitting::{
    AttributeExplanation, CatalogueEntry, CatalogueKind, DamageProfile, EftError, EftLine,
    ExplainTarget, Fit, FitCalculation, FitCatalogue, FitDraft, FitId, FitInput, FitItem,
    FitRepository, FittingEngine, FittingEngineInfo, FittingError, ModuleState, ParsedFit, Pilot,
    SlotKind,
};
use iskworks_core::{InventoryError, WorkspaceId};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ImportCounts, ManufacturingRecipe, SdeError,
    SdeReadRepository, TypeReference, TypeSearchResult,
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
    async fn type_reference(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, TypeReference>, SdeError> {
        let known = [
            (BUSTARD, "Bustard", "Deep Space Transport"),
            (DAMAGE_CONTROL, "Damage Control II", "Damage Control"),
            (AUTOCANNON, "200mm AutoCannon II", "Projectile Weapon"),
            (EMP_S, "EMP S", "Projectile Ammo"),
        ];
        Ok(known
            .into_iter()
            .filter(|(type_id, _, _)| type_ids.contains(type_id))
            .map(|(type_id, name, group)| {
                (
                    type_id,
                    TypeReference {
                        type_name: Some(name.to_string()),
                        group_id: Some(1),
                        group_name: Some(group.to_string()),
                        category_id: Some(7),
                        category_name: Some("Module".to_string()),
                        packaged_volume_m3: None,
                    },
                )
            })
            .collect())
    }
}

const BUSTARD: i64 = 12_731;
const DAMAGE_CONTROL: i64 = 2_048;
const AUTOCANNON: i64 = 2_873;
const EMP_S: i64 = 185;

/// The test router has one workspace, so the fake keeps fits by id alone.
#[derive(Default)]
struct FakeFits(Mutex<Vec<Fit>>);

#[async_trait]
impl FitRepository for FakeFits {
    async fn list_fits(&self, _: WorkspaceId) -> Result<Vec<Fit>, InventoryError> {
        Ok(self.0.lock().unwrap().clone())
    }
    async fn fit(&self, _: WorkspaceId, id: FitId) -> Result<Option<Fit>, InventoryError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|fit| fit.id == id)
            .cloned())
    }
    async fn create_fit(
        &self,
        _: WorkspaceId,
        draft: &FitDraft,
        now: DateTime<Utc>,
    ) -> Result<Fit, InventoryError> {
        let fit = Fit {
            id: FitId(Uuid::new_v4()),
            name: draft.name.trim().to_string(),
            description: draft.description.clone(),
            ship_type_id: draft.ship_type_id,
            items: draft.items.clone(),
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        self.0.lock().unwrap().push(fit.clone());
        Ok(fit)
    }
    async fn update_fit(
        &self,
        _: WorkspaceId,
        id: FitId,
        draft: &FitDraft,
        now: DateTime<Utc>,
    ) -> Result<Option<Fit>, InventoryError> {
        let mut fits = self.0.lock().unwrap();
        let Some(fit) = fits.iter_mut().find(|fit| fit.id == id) else {
            return Ok(None);
        };
        fit.name = draft.name.trim().to_string();
        fit.description = draft.description.clone();
        fit.ship_type_id = draft.ship_type_id;
        fit.items = draft.items.clone();
        fit.updated_at = now;
        Ok(Some(fit.clone()))
    }
    async fn delete_fit(&self, _: WorkspaceId, id: FitId) -> Result<bool, InventoryError> {
        let mut fits = self.0.lock().unwrap();
        let before = fits.len();
        fits.retain(|fit| fit.id != id);
        Ok(fits.len() < before)
    }
    async fn fit_tags_in_use(&self, _: WorkspaceId) -> Result<Vec<String>, InventoryError> {
        let mut tags: Vec<String> = Vec::new();
        for tag in self
            .0
            .lock()
            .unwrap()
            .iter()
            .flat_map(|fit| fit.tags.clone())
        {
            if !tags.iter().any(|held| held.eq_ignore_ascii_case(&tag)) {
                tags.push(tag);
            }
        }
        Ok(tags)
    }
    async fn replace_fit_tags(
        &self,
        _: WorkspaceId,
        id: FitId,
        tags: &[String],
    ) -> Result<Option<Vec<String>>, InventoryError> {
        let mut fits = self.0.lock().unwrap();
        let Some(fit) = fits.iter_mut().find(|fit| fit.id == id) else {
            return Ok(None);
        };
        fit.tags = tags.to_vec();
        Ok(Some(fit.tags.clone()))
    }
    async fn edit_fit_tags(
        &self,
        _: WorkspaceId,
        ids: &[FitId],
        add: &[String],
        remove: &[String],
    ) -> Result<(), InventoryError> {
        for fit in self
            .0
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|fit| ids.contains(&fit.id))
        {
            fit.tags
                .retain(|tag| !remove.iter().any(|gone| gone.eq_ignore_ascii_case(tag)));
            for tag in add {
                if !fit.tags.iter().any(|held| held.eq_ignore_ascii_case(tag)) {
                    fit.tags.push(tag.clone());
                }
            }
            fit.tags.sort_by_key(|tag| tag.to_lowercase());
        }
        Ok(())
    }
}

fn damage_control() -> FitItem {
    FitItem {
        type_id: DAMAGE_CONTROL,
        slot_kind: SlotKind::Low,
        slot_index: Some(0),
        quantity: 1,
        state: ModuleState::Active,
        charge_type_id: None,
        mutation: None,
    }
}

fn loaded_autocannon() -> FitItem {
    FitItem {
        type_id: AUTOCANNON,
        slot_kind: SlotKind::High,
        slot_index: Some(0),
        quantity: 1,
        state: ModuleState::Active,
        charge_type_id: Some(EMP_S),
        mutation: None,
    }
}

struct FakeEngine;

impl FittingEngine for FakeEngine {
    fn catalogue(&self) -> std::sync::Arc<iskworks_core::fitting::FitCatalogue> {
        // The autocannon loads projectile ammo; damage control loads nothing.
        let module = |type_id, charge_groups| CatalogueEntry {
            type_id,
            name: String::new(),
            kind: CatalogueKind::Module,
            group_id: 1,
            slot: None,
            market_group_id: None,
            meta_group_id: None,
            variation_parent: None,
            cpu: 0.0,
            powergrid: 0.0,
            calibration: 0.0,
            rig_size: None,
            hardpoint: None,
            charge_groups,
            charge_size: None,
            fits_ship_types: Vec::new(),
            fits_ship_groups: Vec::new(),
            ship_slots: None,
        };
        std::sync::Arc::new(FitCatalogue::new(
            vec![
                module(AUTOCANNON, vec![83]),
                module(DAMAGE_CONTROL, Vec::new()),
            ],
            Default::default(),
        ))
    }

    fn info(&self) -> FittingEngineInfo {
        FittingEngineInfo {
            data_version: "13.3650286.0".to_string(),
            major: 13,
            build_number: 3_650_286,
        }
    }

    /// `[Bustard, <name>]` parses (a `.` name is none); a line containing
    /// "typo" is an unknown line; anything else isn't EFT.
    fn parse_eft(&self, text: &str) -> Result<ParsedFit, EftError> {
        let typos: Vec<EftLine> = text
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains("typo"))
            .map(|(index, line)| EftLine {
                line: index + 1,
                text: line.to_string(),
            })
            .collect();
        if !typos.is_empty() {
            return Err(EftError::UnknownLines(typos));
        }
        let name = text
            .lines()
            .next()
            .and_then(|header| header.strip_prefix("[Bustard, "))
            .and_then(|rest| rest.strip_suffix(']'))
            .ok_or_else(|| EftError::Invalid("invalid EFT header".to_string()))?;
        Ok(ParsedFit {
            name: (name != ".").then(|| name.to_string()),
            fit: FitInput {
                ship_type_id: BUSTARD,
                items: vec![damage_control()],
            },
        })
    }

    fn write_eft(&self, name: &str, fit: &FitInput) -> Result<String, FittingError> {
        Ok(format!(
            "[{}, {name}]\n{} items\n",
            fit.ship_type_id,
            fit.items.len()
        ))
    }

    fn calculate(
        &self,
        _: &FitInput,
        _: &Pilot,
        _: &DamageProfile,
    ) -> Result<FitCalculation, FittingError> {
        unimplemented!("meta does not calculate")
    }

    fn explain(
        &self,
        _: &FitInput,
        _: &Pilot,
        _: &DamageProfile,
        _: ExplainTarget,
        _: &str,
    ) -> Result<AttributeExplanation, FittingError> {
        unimplemented!()
    }
}

fn state() -> AppState {
    AppState::new(Arc::new(configured_workspace("Fittings Test")))
        .with_sde_repository(Arc::new(FakeSde))
        .with_fit_repository(Arc::new(FakeFits::default()))
}

fn app() -> axum::Router {
    build_router(state().with_fitting_engine(Arc::new(FakeEngine)))
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    send(&app, Request::get(uri).body(Body::empty()).unwrap()).await
}

async fn json(app: &axum::Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

fn fit_body(name: &str, items: Vec<FitItem>) -> Value {
    json!({ "name": name, "description": "", "shipTypeId": BUSTARD, "items": items })
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

#[tokio::test]
async fn a_fit_is_created_listed_read_replaced_and_deleted() {
    let app = app();
    let (status, created) = json(
        &app,
        "POST",
        "/api/fittings",
        fit_body("  Null Transport ", vec![loaded_autocannon()]),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["name"], "Null Transport");
    assert_eq!(
        created["ship"],
        json!({ "typeId": BUSTARD, "name": "Bustard", "groupName": "Deep Space Transport" })
    );
    assert_eq!(created["items"][0]["name"], "200mm AutoCannon II");
    assert_eq!(created["items"][0]["groupName"], "Projectile Weapon");
    assert_eq!(created["items"][0]["chargeName"], "EMP S");
    assert_eq!(created["items"][0]["loadsCharges"], true);
    assert_eq!(created["items"][0]["slotKind"], "high");
    let id = created["id"].as_str().unwrap().to_string();

    let (status, list) = get(app.clone(), "/api/fittings").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "Null Transport");
    assert_eq!(list[0]["ship"]["name"], "Bustard");
    assert_eq!(list[0]["itemCount"], 1);

    let (status, read) = get(app.clone(), &format!("/api/fittings/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read, created);

    let (status, replaced) = json(
        &app,
        "PUT",
        &format!("/api/fittings/{id}"),
        fit_body("Renamed", vec![damage_control()]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replaced["name"], "Renamed");
    assert_eq!(replaced["items"][0]["name"], "Damage Control II");
    assert_eq!(replaced["items"][0]["chargeName"], Value::Null);
    assert_eq!(replaced["items"][0]["loadsCharges"], false);

    let (status, _) = send(
        &app,
        Request::delete(format!("/api/fittings/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = get(app.clone(), &format!("/api/fittings/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_misshapen_fit_is_refused_and_nothing_is_saved() {
    let app = app();
    let (status, body) = json(&app, "POST", "/api/fittings", fit_body("   ", Vec::new())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation_failed");

    let mut no_position = damage_control();
    no_position.slot_index = None;
    let (status, _) = json(
        &app,
        "POST",
        "/api/fittings",
        fit_body("Fit", vec![no_position]),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (_, list) = get(app.clone(), "/api/fittings").await;
    assert_eq!(list, json!([]));
}

#[tokio::test]
async fn parsing_previews_a_fit_without_saving_it() {
    let app = app();
    let (status, preview) = json(
        &app,
        "POST",
        "/api/fittings/parse",
        json!({ "text": "[Bustard, .]\nDamage Control II" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["fit"]["name"], "Bustard fit");
    assert_eq!(preview["fit"]["ship"]["name"], "Bustard");
    assert_eq!(preview["fit"]["items"][0]["name"], "Damage Control II");
    assert_eq!(preview["unknownLines"], json!([]));
    assert_eq!(preview["error"], Value::Null);

    let (_, list) = get(app.clone(), "/api/fittings").await;
    assert_eq!(list, json!([]));
}

#[tokio::test]
async fn parsing_lists_unknown_lines_and_says_why_text_is_not_a_fit() {
    let app = app();
    let (status, preview) = json(
        &app,
        "POST",
        "/api/fittings/parse",
        json!({ "text": "[Bustard, .]\nDamage typo II\nWarp typo" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["fit"], Value::Null);
    assert_eq!(
        preview["unknownLines"],
        json!([{ "line": 2, "text": "Damage typo II" }, { "line": 3, "text": "Warp typo" }])
    );

    let (status, preview) = json(
        &app,
        "POST",
        "/api/fittings/parse",
        json!({ "text": "hello" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["fit"], Value::Null);
    assert_eq!(preview["error"], "invalid EFT header");
}

#[tokio::test]
async fn importing_names_the_fit_from_the_body_the_text_or_the_ship() {
    let app = app();
    for (body, expected) in [
        (json!({ "text": "[Bustard, .]" }), "Bustard fit"),
        (json!({ "text": "[Bustard, Hauler]" }), "Hauler"),
        (
            json!({ "text": "[Bustard, Hauler]", "name": "Mine" }),
            "Mine",
        ),
        (
            json!({ "text": "[Bustard, Hauler]", "name": "  " }),
            "Hauler",
        ),
    ] {
        let (status, created) = json(&app, "POST", "/api/fittings/import", body).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(created["name"], expected);
        assert_eq!(created["items"][0]["name"], "Damage Control II");
    }
}

#[tokio::test]
async fn importing_text_with_unknown_lines_is_refused_with_them() {
    let app = app();
    let (status, body) = json(
        &app,
        "POST",
        "/api/fittings/import",
        json!({ "text": "[Bustard, .]\nDamage typo II" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("line 2: Damage typo II"),
        "{body}"
    );
    let (_, list) = get(app.clone(), "/api/fittings").await;
    assert_eq!(list, json!([]));
}

#[tokio::test]
async fn a_fit_exports_as_eft_text() {
    let app = app();
    let (_, created) = json(
        &app,
        "POST",
        "/api/fittings",
        fit_body("Null Transport", vec![damage_control()]),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("/api/fittings/{id}/export"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        String::from_utf8(bytes.to_vec()).unwrap(),
        "[12731, Null Transport]\n1 items\n"
    );
}

#[tokio::test]
async fn text_needs_the_engine_but_saved_fits_do_not() {
    let app = build_router(state());
    let (status, created) = json(
        &app,
        "POST",
        "/api/fittings",
        fit_body("Null Transport", vec![damage_control()]),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();

    for (method, uri, body) in [
        (
            "POST",
            "/api/fittings/parse".to_string(),
            json!({ "text": "[Bustard, .]" }),
        ),
        (
            "POST",
            "/api/fittings/import".to_string(),
            json!({ "text": "[Bustard, .]" }),
        ),
    ] {
        let (status, body) = json(&app, method, &uri, body).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert_eq!(body["error"]["code"], "fitting_engine_unavailable");
    }
    let (status, body) = get(app.clone(), &format!("/api/fittings/{id}/export")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "fitting_engine_unavailable");
}

#[tokio::test]
async fn without_the_engine_no_item_is_marked_as_loading_charges() {
    let app = build_router(state());
    let (status, created) = json(
        &app,
        "POST",
        "/api/fittings",
        fit_body("No engine", vec![loaded_autocannon()]),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["items"][0]["loadsCharges"], false);
}
