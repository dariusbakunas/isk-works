//! A new producer Build starts from the owner's best synced blueprint: an
//! Original first, else a Copy -- unless the consumer's row already chose
//! one. Without it the producer plans at ME 0 and overstates materials.

use super::*;
use iskworks_core::{BlueprintKind, BlueprintObservation, WorkspaceId};

/// The hull-section blueprint (90_002) the parent's row 90_001 builds with.
fn observation(owner_id: OwnerId, kind: BlueprintKind, me: u8) -> BlueprintObservation {
    let now = chrono::Utc::now();
    BlueprintObservation {
        id: uuid::Uuid::new_v4(),
        workspace_id: WorkspaceId::new(),
        owner_id,
        owner_name: "Pilot".to_string(),
        eve_item_id: i64::from(me),
        blueprint_type_id: 90_002,
        blueprint_name: "Rifter Hull Section Blueprint".to_string(),
        kind,
        material_efficiency: me,
        time_efficiency: 2 * me,
        licensed_runs: (kind == BlueprintKind::Copy).then_some(10),
        location_id: 60_003_760,
        location_flag: "Hangar".to_string(),
        location_name: None,
        observed_at: now,
        imported_at: now,
    }
}

fn app(parent: Build, owner_id: OwnerId, observations: Vec<BlueprintObservation>) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            blueprint_observations: observations
                .into_iter()
                .map(|observation| (observation.id, observation))
                .collect(),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(Arc::new(EmptyInventoryRepository))
        .with_production_repository(Arc::new(FixtureProductionRepository {
            type_id: 0,
            available_to_this_build: 0,
            average_historical_unit_cost: None,
        })),
    )
}

/// Creates the producer for row 90_001 and returns its blueprint selection.
async fn producer_blueprint(app: axum::Router, parent_id: BuildId) -> serde_json::Value {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/linked-builds", parent_id.0))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"componentTypeId": 90001}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    body["draftPlanning"]["input"]["blueprintSelection"].clone()
}

#[tokio::test]
async fn a_new_producer_uses_an_original_over_a_better_copy() {
    let owner_id = OwnerId::new();
    let parent = parent_build_resolving_hull_section_to_build(WorkspaceId::new(), owner_id);
    let parent_id = parent.id;
    let copy = observation(owner_id, BlueprintKind::Copy, 10);
    let original = observation(owner_id, BlueprintKind::Original, 8);
    let original_id = original.id;

    let selection =
        producer_blueprint(app(parent, owner_id, vec![copy, original]), parent_id).await;

    assert_eq!(selection["mode"], "observedAsset");
    assert_eq!(selection["observationId"], original_id.to_string());
    assert_eq!(selection["kind"], "original");
    assert_eq!(selection["materialEfficiency"], 8);
    assert_eq!(selection["timeEfficiency"], 16);
}

#[tokio::test]
async fn a_new_producer_uses_the_best_original() {
    let owner_id = OwnerId::new();
    let parent = parent_build_resolving_hull_section_to_build(WorkspaceId::new(), owner_id);
    let parent_id = parent.id;
    let worse = observation(owner_id, BlueprintKind::Original, 5);
    let better = observation(owner_id, BlueprintKind::Original, 10);
    let better_id = better.id;

    let selection = producer_blueprint(app(parent, owner_id, vec![worse, better]), parent_id).await;

    assert_eq!(selection["observationId"], better_id.to_string());
}

#[tokio::test]
async fn without_an_original_a_new_producer_uses_the_best_copy() {
    let owner_id = OwnerId::new();
    let parent = parent_build_resolving_hull_section_to_build(WorkspaceId::new(), owner_id);
    let parent_id = parent.id;
    let worse = observation(owner_id, BlueprintKind::Copy, 4);
    let better = observation(owner_id, BlueprintKind::Copy, 9);
    let better_id = better.id;

    let selection = producer_blueprint(app(parent, owner_id, vec![worse, better]), parent_id).await;

    assert_eq!(selection["observationId"], better_id.to_string());
    assert_eq!(selection["kind"], "copy");
    assert_eq!(selection["licensedRuns"], 10);
}

#[tokio::test]
async fn the_rows_own_blueprint_choice_beats_the_default() {
    let owner_id = OwnerId::new();
    let mut parent = parent_build_resolving_hull_section_to_build(WorkspaceId::new(), owner_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .component_resolutions[0]
        .blueprint_selection = Some(iskworks_core::BlueprintSelection::Manual {
        kind: BlueprintKind::Original,
        material_efficiency: 3,
        time_efficiency: 6,
        licensed_runs: None,
        notes: String::new(),
    });
    let parent_id = parent.id;
    let original = observation(owner_id, BlueprintKind::Original, 10);

    let selection = producer_blueprint(app(parent, owner_id, vec![original]), parent_id).await;

    assert_eq!(selection["mode"], "manual");
    assert_eq!(selection["materialEfficiency"], 3);
}

#[tokio::test]
async fn unsynced_kinds_and_other_owners_blueprints_are_never_the_default() {
    let owner_id = OwnerId::new();
    let parent = parent_build_resolving_hull_section_to_build(WorkspaceId::new(), owner_id);
    let parent_id = parent.id;
    let unknown = observation(owner_id, BlueprintKind::Unknown, 10);
    let someone_elses = observation(OwnerId::new(), BlueprintKind::Original, 10);

    let selection = producer_blueprint(
        app(parent, owner_id, vec![unknown, someone_elses]),
        parent_id,
    )
    .await;

    assert!(selection.is_null(), "no usable blueprint: {selection}");
}
