//! Workspace facility defaults applied to new producer Builds (issue #12):
//! row override → most specific target default → role fallback → the
//! consumer's own facility → none.

use super::*;
use iskworks_core::facility_defaults::FacilityDefault;
use iskworks_core::{FacilityProfileId, FacilityRole, IndustryFacilityProfile, WorkspaceId};

fn profile(role: FacilityRole) -> IndustryFacilityProfile {
    fixture_facility_profile(role)
}

fn default_for(target: Option<i64>, facility: &IndustryFacilityProfile) -> FacilityDefault {
    FacilityDefault {
        role: facility.role,
        target_filter_id: target,
        facility_profile_id: facility.id,
        rig_covered_at_assignment: true,
    }
}

/// The hull-section parent (whose row 90_001 is set to Produce), with the
/// parent's own manufacturing slot on `parent_facility` when given.
fn parent_on(owner_id: OwnerId, parent_facility: Option<FacilityProfileId>) -> Build {
    let mut parent = parent_build_resolving_hull_section_to_build(WorkspaceId::new(), owner_id);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .manufacturing_facility =
        parent_facility.map(
            |facility_profile_id| iskworks_core::FacilityPreviewCommand {
                facility_profile_id,
                blueprint_me: 0,
                blueprint_te: 0,
                estimated_item_value: None,
            },
        );
    parent
}

fn app(
    parent: Build,
    owner_id: OwnerId,
    profiles: Vec<IndustryFacilityProfile>,
    facility_defaults: Vec<FacilityDefault>,
) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            facility_profiles: profiles
                .into_iter()
                .map(|profile| (profile.id, profile))
                .collect(),
            facility_defaults,
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

/// Creates the producer for row 90_001 and returns its manufacturing
/// facility id, if any.
async fn producer_facility(app: axum::Router, parent_id: iskworks_core::BuildId) -> Option<String> {
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
    body["draftPlanning"]["input"]["manufacturingFacility"]["facilityProfileId"]
        .as_str()
        .map(str::to_string)
}

#[tokio::test]
async fn a_new_producer_takes_its_target_default_over_the_consumers_facility() {
    let owner_id = OwnerId::new();
    let parent_facility = profile(FacilityRole::Manufacturing);
    let components = profile(FacilityRole::Manufacturing);
    let ships = profile(FacilityRole::Manufacturing);
    let parent = parent_on(owner_id, Some(parent_facility.id));
    let parent_id = parent.id;
    let defaults = vec![
        default_for(Some(3), &ships),
        default_for(Some(14), &components),
    ];
    let app = app(
        parent,
        owner_id,
        vec![parent_facility, components.clone(), ships],
        defaults,
    );

    assert_eq!(
        producer_facility(app, parent_id).await,
        Some(components.id.0.to_string())
    );
}

#[tokio::test]
async fn the_rows_own_facility_override_beats_the_default() {
    let owner_id = OwnerId::new();
    let overridden = profile(FacilityRole::Manufacturing);
    let components = profile(FacilityRole::Manufacturing);
    let mut parent = parent_on(owner_id, None);
    parent
        .draft_planning
        .as_mut()
        .unwrap()
        .input
        .component_resolutions[0]
        .facility_override = Some(iskworks_core::ComponentFacilityOverride {
        facility_profile_id: overridden.id,
    });
    let parent_id = parent.id;
    let app = app(
        parent,
        owner_id,
        vec![overridden.clone(), components.clone()],
        vec![default_for(Some(14), &components)],
    );

    assert_eq!(
        producer_facility(app, parent_id).await,
        Some(overridden.id.0.to_string())
    );
}

#[tokio::test]
async fn the_role_fallback_applies_when_no_matching_target_has_a_default() {
    let owner_id = OwnerId::new();
    let parent_facility = profile(FacilityRole::Manufacturing);
    let ships = profile(FacilityRole::Manufacturing);
    let fallback = profile(FacilityRole::Manufacturing);
    let reaction_fallback = profile(FacilityRole::Reaction);
    let parent = parent_on(owner_id, Some(parent_facility.id));
    let parent_id = parent.id;
    let app = app(
        parent,
        owner_id,
        vec![
            parent_facility,
            ships.clone(),
            fallback.clone(),
            reaction_fallback.clone(),
        ],
        vec![
            default_for(Some(3), &ships),
            default_for(None, &fallback),
            default_for(None, &reaction_fallback),
        ],
    );

    assert_eq!(
        producer_facility(app, parent_id).await,
        Some(fallback.id.0.to_string())
    );
}

#[tokio::test]
async fn an_archived_default_falls_through_to_inheriting_the_consumers_facility() {
    let owner_id = OwnerId::new();
    let parent_facility = profile(FacilityRole::Manufacturing);
    let mut archived = profile(FacilityRole::Manufacturing);
    archived.archived_at = Some(chrono::Utc::now());
    let parent = parent_on(owner_id, Some(parent_facility.id));
    let parent_id = parent.id;
    let app = app(
        parent,
        owner_id,
        vec![parent_facility.clone(), archived.clone()],
        vec![default_for(Some(14), &archived)],
    );

    assert_eq!(
        producer_facility(app, parent_id).await,
        Some(parent_facility.id.0.to_string())
    );
}

#[tokio::test]
async fn without_defaults_a_new_producer_still_inherits_the_consumers_facility() {
    let owner_id = OwnerId::new();
    let parent_facility = profile(FacilityRole::Manufacturing);
    let parent = parent_on(owner_id, Some(parent_facility.id));
    let parent_id = parent.id;
    let app = app(parent, owner_id, vec![parent_facility.clone()], Vec::new());

    assert_eq!(
        producer_facility(app, parent_id).await,
        Some(parent_facility.id.0.to_string())
    );
}
