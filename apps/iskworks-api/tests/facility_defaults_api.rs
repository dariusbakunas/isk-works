//! `GET`/`PUT /api/industry/facility-defaults` over in-memory fakes.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::facility_defaults::{
    FacilityDefault, FacilityDefaultChange, FacilityDefaultsRepository,
};
use iskworks_core::{
    FacilityError, FacilityKind, FacilityProfileId, FacilityRig, FacilityRole,
    IndustryFacilityProfile, Money, RigApplicability, RigTargetFilter, SecurityClass, WorkspaceId,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, IndustryTargetFilter, ManufacturingRecipe, RigActivity,
    SdeError, TypeSearchResult,
};
use rust_decimal::Decimal;
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

#[derive(Default)]
struct FakeDefaults {
    profiles: Vec<IndustryFacilityProfile>,
    defaults: Mutex<Vec<FacilityDefault>>,
    invention: Mutex<Option<FacilityProfileId>>,
}

#[async_trait]
impl FacilityDefaultsRepository for FakeDefaults {
    async fn list_facility_defaults(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<Vec<FacilityDefault>, FacilityError> {
        Ok(self.defaults.lock().unwrap().clone())
    }

    async fn list_facility_profiles(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<Vec<IndustryFacilityProfile>, FacilityError> {
        Ok(self.profiles.clone())
    }

    async fn apply_facility_default_changes(
        &self,
        _workspace_id: WorkspaceId,
        changes: Vec<FacilityDefaultChange>,
    ) -> Result<(), FacilityError> {
        let mut defaults = self.defaults.lock().unwrap();
        for change in changes {
            let (role, target) = match &change {
                FacilityDefaultChange::Set(default) => (default.role, default.target_filter_id),
                FacilityDefaultChange::Clear {
                    role,
                    target_filter_id,
                } => (*role, *target_filter_id),
            };
            defaults
                .retain(|default| !(default.role == role && default.target_filter_id == target));
            if let FacilityDefaultChange::Set(default) = change {
                defaults.push(default);
            }
        }
        Ok(())
    }

    async fn invention_facility_default(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<Option<FacilityProfileId>, FacilityError> {
        Ok(*self.invention.lock().unwrap())
    }

    async fn set_invention_facility_default(
        &self,
        _workspace_id: WorkspaceId,
        facility_profile_id: Option<FacilityProfileId>,
    ) -> Result<(), FacilityError> {
        *self.invention.lock().unwrap() = facility_profile_id;
        Ok(())
    }
}

/// A frigate (group 25, category 6): in "Small T1 Ships" and "Ships".
const FRIGATE: i64 = 594;
/// A module (category 7): in no target.
const MODULE: i64 = 2048;

struct FakeSde;

#[async_trait]
impl iskworks_sde::FakeSdeRead for FakeSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(None)
    }

    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn manufacturing_recipe(
        &self,
        _blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        Ok(None)
    }

    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn industry_target_filters_by_activity(
        &self,
    ) -> Result<Vec<(RigActivity, IndustryTargetFilter)>, SdeError> {
        let filter = |filter_id: i64, name: &str, category_ids: Vec<i64>, group_ids: Vec<i64>| {
            IndustryTargetFilter {
                filter_id,
                name: name.to_string(),
                category_ids,
                group_ids,
            }
        };
        Ok(vec![
            (
                RigActivity::Manufacturing,
                filter(3, "Ships", vec![6, 32], vec![]),
            ),
            (
                RigActivity::Manufacturing,
                filter(5, "Small T1 Ships", vec![], vec![25, 31, 420]),
            ),
            (
                RigActivity::Reaction,
                filter(16, "Hybrid Reactions", vec![], vec![974]),
            ),
        ])
    }

    async fn type_classifications(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, iskworks_sde::SdeTypeClassification>, SdeError> {
        Ok(type_ids
            .iter()
            .filter_map(|type_id| {
                let (category_id, group_id) = match *type_id {
                    FRIGATE => (6, 25),
                    MODULE => (7, 60),
                    _ => return None,
                };
                Some((
                    *type_id,
                    iskworks_sde::SdeTypeClassification {
                        category_id: Some(category_id),
                        category_name: None,
                        group_id: Some(group_id),
                        group_name: None,
                    },
                ))
            })
            .collect())
    }

    async fn group_categories(&self, group_ids: &[i64]) -> Result<BTreeMap<i64, i64>, SdeError> {
        Ok(group_ids
            .iter()
            .map(|group| (*group, if *group == 974 { 4 } else { 6 }))
            .collect())
    }
}

fn id(n: u128) -> FacilityProfileId {
    FacilityProfileId(Uuid::from_u128(n))
}

fn profile(
    n: u128,
    name: &str,
    role: FacilityRole,
    rigs: Vec<FacilityRig>,
) -> IndustryFacilityProfile {
    let now = chrono::Utc::now();
    IndustryFacilityProfile {
        id: id(n),
        workspace_id: WorkspaceId::new(),
        name: name.to_string(),
        kind: FacilityKind::Manual,
        role,
        structure_id: None,
        structure_type_id: None,
        structure_type_name: String::new(),
        solar_system_id: None,
        solar_system_name: String::new(),
        security_class: SecurityClass::HighSec,
        material_reduction_percent: Decimal::ONE,
        time_reduction_percent: Decimal::from(15),
        job_cost_reduction_percent: Decimal::ZERO,
        facility_tax_percent: Decimal::ZERO,
        scc_surcharge_percent: Decimal::ZERO,
        alliance_surcharge_percent: Decimal::ZERO,
        fixed_supplemental_cost: Money::parse("0").unwrap(),
        manual_system_cost_index: None,
        notes: String::new(),
        rigs,
        archived_at: None,
        revision: 1,
        created_at: now,
        updated_at: now,
    }
}

fn small_ship_rig() -> FacilityRig {
    let filter = RigTargetFilter::Restricted {
        category_ids: BTreeSet::new(),
        group_ids: [25, 31, 420].into_iter().collect(),
    };
    FacilityRig {
        slot_number: 1,
        type_id: 43_920,
        type_name: "Standup M-Set Basic Small Ship Manufacturing Material Efficiency I".into(),
        material_reduction_percent: Decimal::from(2),
        time_reduction_percent: Decimal::from(20),
        applicability: RigApplicability {
            material: filter.clone(),
            time: filter,
        },
        science: Default::default(),
    }
}

fn app(repository: Arc<FakeDefaults>) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace("Defaults")))
            .with_facility_defaults_repository(repository)
            .with_sde_repository(Arc::new(FakeSde)),
    )
}

async fn send(
    app: axum::Router,
    method: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    send_to(app, method, "", body).await
}

/// `suffix` is appended to `/api/industry/facility-defaults`.
async fn send_to(
    app: axum::Router,
    method: &str,
    suffix: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(format!("/api/industry/facility-defaults{suffix}"))
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let body =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    (status, body)
}

fn row<'a>(
    view: &'a serde_json::Value,
    role: &str,
    filter_id: Option<i64>,
) -> &'a serde_json::Value {
    view["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["role"] == role && row["filterId"] == serde_json::json!(filter_id))
        .unwrap_or_else(|| panic!("no {role} row {filter_id:?} in {view}"))
}

fn rigged_workspace() -> Arc<FakeDefaults> {
    Arc::new(FakeDefaults {
        profiles: vec![
            profile(1, "Raitaru", FacilityRole::Manufacturing, vec![]),
            profile(
                2,
                "Azbel",
                FacilityRole::Manufacturing,
                vec![small_ship_rig()],
            ),
            profile(3, "Tatara", FacilityRole::Reaction, vec![]),
        ],
        ..Default::default()
    })
}

#[tokio::test]
async fn get_lists_every_target_with_suggestions_bonuses_and_statuses() {
    let (status, view) = send(app(rigged_workspace()), "GET", None).await;

    assert_eq!(status, StatusCode::OK, "{view}");
    let names: Vec<_> = view["targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "Ships",
            "Small T1 Ships",
            "Everything else",
            "Hybrid Reactions",
            "Everything else"
        ]
    );
    let small = row(&view, "manufacturing", Some(5));
    assert_eq!(small["status"], "none");
    assert_eq!(small["default"], serde_json::Value::Null);
    assert_eq!(small["suggestion"]["facilityName"], "Azbel");
    // 1 - 0.99 * 0.98 = 2.98%; 1 - 0.85 * 0.80 = 32%
    assert_eq!(small["suggestion"]["materialBonusPercent"], "2.98");
    assert_eq!(small["suggestion"]["timeBonusPercent"], "32");
    // No rig covers all Ships, so no suggestion there.
    assert_eq!(
        row(&view, "manufacturing", Some(3))["suggestion"],
        serde_json::Value::Null
    );
    // The fallback suggestion ranks base bonuses alone (a tie, broken by name).
    assert_eq!(
        row(&view, "manufacturing", None)["suggestion"]["facilityName"],
        "Azbel"
    );
    let azbel_small = view["bonuses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|bonus| bonus["facilityProfileId"] == id(2).0.to_string() && bonus["filterId"] == 5)
        .unwrap();
    assert_eq!(azbel_small["covered"], true);
}

#[tokio::test]
async fn put_assigns_and_clears_defaults_and_returns_the_new_view() {
    let repository = rigged_workspace();
    let (status, view) = send(
        app(repository.clone()),
        "PUT",
        Some(serde_json::json!({"assignments": [
            {"role": "manufacturing", "filterId": 5, "facilityProfileId": id(2).0},
            {"role": "manufacturing", "filterId": null, "facilityProfileId": id(1).0},
        ]})),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{view}");
    let small = row(&view, "manufacturing", Some(5));
    assert_eq!(small["status"], "set");
    assert_eq!(small["default"]["facilityName"], "Azbel");
    assert_eq!(small["suggestion"], serde_json::Value::Null);
    assert_eq!(row(&view, "manufacturing", Some(3))["status"], "fallback");
    // The fallback (Raitaru) still has a better-ranked alternative.
    assert_eq!(
        row(&view, "manufacturing", None)["suggestion"]["facilityName"],
        "Azbel"
    );

    assert_eq!(row(&view, "reaction", Some(16))["status"], "none");
    let stored = repository.defaults.lock().unwrap().clone();
    assert!(stored
        .iter()
        .any(|default| default.target_filter_id == Some(5) && default.rig_covered_at_assignment));
    assert!(stored
        .iter()
        .any(|default| default.target_filter_id.is_none() && !default.rig_covered_at_assignment));

    let (status, view) = send(
        app(repository.clone()),
        "PUT",
        Some(serde_json::json!({"assignments": [
            {"role": "manufacturing", "filterId": null, "facilityProfileId": null},
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(row(&view, "manufacturing", None)["status"], "none");
    assert_eq!(row(&view, "manufacturing", Some(3))["status"], "none");
}

#[tokio::test]
async fn a_suggestion_matching_the_fallback_already_in_use_is_hidden() {
    let repository = rigged_workspace();
    repository.defaults.lock().unwrap().push(FacilityDefault {
        role: FacilityRole::Manufacturing,
        target_filter_id: None,
        facility_profile_id: id(2),
        rig_covered_at_assignment: false,
    });

    let (_, view) = send(app(repository), "GET", None).await;

    // Small T1 Ships already gets Azbel through the fallback.
    let small = row(&view, "manufacturing", Some(5));
    assert_eq!(small["status"], "fallback");
    assert_eq!(small["suggestion"], serde_json::Value::Null);
}

#[tokio::test]
async fn a_default_whose_rigs_no_longer_cover_its_target_is_flagged() {
    let mut repository = FakeDefaults {
        profiles: vec![profile(2, "Azbel", FacilityRole::Manufacturing, vec![])],
        ..Default::default()
    };
    repository.defaults = Mutex::new(vec![FacilityDefault {
        role: FacilityRole::Manufacturing,
        target_filter_id: Some(5),
        facility_profile_id: id(2),
        rig_covered_at_assignment: true,
    }]);

    let (_, view) = send(app(Arc::new(repository)), "GET", None).await;

    assert_eq!(
        row(&view, "manufacturing", Some(5))["status"],
        "coverageLost"
    );
}

#[tokio::test]
async fn put_rejects_a_wrong_role_an_archived_facility_or_an_unknown_target() {
    let mut workspace = FakeDefaults {
        profiles: vec![
            profile(1, "Raitaru", FacilityRole::Manufacturing, vec![]),
            profile(3, "Tatara", FacilityRole::Reaction, vec![]),
        ],
        ..Default::default()
    };
    workspace.profiles[0].archived_at = Some(chrono::Utc::now());
    let repository = Arc::new(workspace);
    for assignment in [
        serde_json::json!({"role": "manufacturing", "filterId": 3, "facilityProfileId": id(3).0}),
        serde_json::json!({"role": "manufacturing", "filterId": 3, "facilityProfileId": id(1).0}),
        serde_json::json!({"role": "manufacturing", "filterId": 16, "facilityProfileId": id(3).0}),
        serde_json::json!({"role": "reaction", "filterId": 99, "facilityProfileId": id(3).0}),
    ] {
        let (status, body) = send(
            app(repository.clone()),
            "PUT",
            Some(serde_json::json!({"assignments": [assignment]})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{assignment} -> {body}");
    }
    assert!(repository.defaults.lock().unwrap().is_empty());
}

#[tokio::test]
async fn put_rejects_an_oversized_assignment_batch() {
    let repository = rigged_workspace();
    let assignments: Vec<_> = (0..101)
        .map(|_| serde_json::json!({"role": "manufacturing", "filterId": null, "facilityProfileId": null}))
        .collect();

    let (status, body) = send(
        app(repository.clone()),
        "PUT",
        Some(serde_json::json!({ "assignments": assignments })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

fn lab_rig() -> FacilityRig {
    FacilityRig {
        slot_number: 1,
        type_id: 43_879,
        type_name: "Standup M-Set Invention Accelerator II".into(),
        material_reduction_percent: Decimal::ZERO,
        time_reduction_percent: Decimal::ZERO,
        applicability: RigApplicability::default(),
        science: iskworks_core::RigScienceBonuses {
            invention_cost_reduction_percent: Decimal::from(10),
            invention_time_reduction_percent: Decimal::from(50),
            ..Default::default()
        },
    }
}

fn archived(mut profile: IndustryFacilityProfile) -> IndustryFacilityProfile {
    profile.archived_at = Some(chrono::Utc::now());
    profile
}

#[tokio::test]
async fn the_invention_default_is_set_shown_with_its_bonus_and_cleared() {
    let repository = Arc::new(FakeDefaults {
        profiles: vec![
            profile(3, "Tatara", FacilityRole::Reaction, vec![]),
            profile(1, "Raitaru", FacilityRole::Manufacturing, vec![]),
            profile(2, "Lab", FacilityRole::Manufacturing, vec![lab_rig()]),
            archived(profile(4, "Old", FacilityRole::Manufacturing, vec![])),
        ],
        ..Default::default()
    });

    let (status, view) = send(app(repository.clone()), "GET", None).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["invention"]["default"], serde_json::Value::Null);
    let names: Vec<_> = view["invention"]["facilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|facility| facility["facilityName"].as_str().unwrap())
        .collect();
    // Manufacturing profiles first; the archived one is not offered.
    assert_eq!(names, vec!["Raitaru", "Lab", "Tatara"]);

    let (status, view) = send_to(
        app(repository.clone()),
        "PUT",
        "/invention",
        Some(serde_json::json!({ "facilityProfileId": id(2).0 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{view}");
    let default = &view["invention"]["default"];
    assert_eq!(default["facilityName"], "Lab");
    // Job cost 0% and time 15% from the structure, then the rig.
    assert_eq!(default["costBonusPercent"], "10");
    assert_eq!(default["timeBonusPercent"], "57.5");
    // A reaction profile has no structure bonus for science jobs.
    assert_eq!(view["invention"]["facilities"][2]["timeBonusPercent"], "0");

    let (status, view) = send_to(
        app(repository),
        "PUT",
        "/invention",
        Some(serde_json::json!({ "facilityProfileId": null })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["invention"]["default"], serde_json::Value::Null);
}

#[tokio::test]
async fn the_invention_default_rejects_an_archived_or_unknown_facility() {
    let repository = Arc::new(FakeDefaults {
        profiles: vec![archived(profile(
            4,
            "Old",
            FacilityRole::Manufacturing,
            vec![],
        ))],
        ..Default::default()
    });
    for facility in [id(4), id(9)] {
        let (status, body) = send_to(
            app(repository.clone()),
            "PUT",
            "/invention",
            Some(serde_json::json!({ "facilityProfileId": facility.0 })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    assert_eq!(*repository.invention.lock().unwrap(), None);
}

fn manufacturing_default(target: Option<i64>, n: u128) -> FacilityDefault {
    FacilityDefault {
        role: FacilityRole::Manufacturing,
        target_filter_id: target,
        facility_profile_id: id(n),
        rig_covered_at_assignment: false,
    }
}

#[tokio::test]
async fn product_defaults_follow_the_most_specific_target_then_the_fallback() {
    let repository = Arc::new(FakeDefaults {
        profiles: vec![
            profile(1, "Raitaru", FacilityRole::Manufacturing, vec![]),
            profile(2, "Azbel", FacilityRole::Manufacturing, vec![]),
            profile(5, "Sotiyo", FacilityRole::Manufacturing, vec![]),
            profile(3, "Tatara", FacilityRole::Reaction, vec![]),
        ],
        defaults: Mutex::new(vec![
            manufacturing_default(Some(3), 1),
            manufacturing_default(Some(5), 2),
            manufacturing_default(None, 5),
        ]),
        invention: Mutex::new(Some(id(3))),
    });
    let defaults_for = |query: &'static str| {
        let repository = repository.clone();
        async move {
            let (status, body) = send_to(app(repository), "GET", query, None).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            body
        }
    };

    let frigate = defaults_for("/for-product?productTypeId=594").await;
    assert_eq!(frigate["manufacturingFacilityId"], id(2).0.to_string());
    assert_eq!(frigate["inventionFacilityId"], id(3).0.to_string());

    let module = defaults_for("/for-product?productTypeId=2048").await;
    assert_eq!(module["manufacturingFacilityId"], id(5).0.to_string());

    // Without a product only the invention default is known.
    let none = defaults_for("/for-product").await;
    assert_eq!(none["manufacturingFacilityId"], serde_json::Value::Null);
    assert_eq!(none["inventionFacilityId"], id(3).0.to_string());
}

#[tokio::test]
async fn product_defaults_skip_a_facility_that_is_no_longer_usable() {
    let repository = Arc::new(FakeDefaults {
        profiles: vec![
            profile(1, "Raitaru", FacilityRole::Manufacturing, vec![]),
            archived(profile(2, "Azbel", FacilityRole::Manufacturing, vec![])),
        ],
        defaults: Mutex::new(vec![
            manufacturing_default(Some(3), 1),
            manufacturing_default(Some(5), 2),
        ]),
        invention: Mutex::new(Some(id(2))),
    });

    let (status, body) = send_to(
        app(repository),
        "GET",
        "/for-product?productTypeId=594",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    // The archived Small T1 Ships default gives way to the Ships one.
    assert_eq!(body["manufacturingFacilityId"], id(1).0.to_string());
    assert_eq!(body["inventionFacilityId"], serde_json::Value::Null);
}
