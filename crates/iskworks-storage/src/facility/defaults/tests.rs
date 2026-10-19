use iskworks_core::facility_defaults::{
    FacilityDefault, FacilityDefaultChange, FacilityDefaultsRepository,
};
use iskworks_core::{
    CreateFacilityProfileCommand, FacilityError, FacilityKind, FacilityRole,
    IndustryFacilityProfile, SecurityClass, WorkspaceId,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::facility::PgFacilityRepository;

async fn workspace(pool: &PgPool) -> WorkspaceId {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let now = crate::db_now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Defaults Test',$2,$3,$3)")
        .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Defaults Test',true,$3,$3)")
        .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    WorkspaceId(workspace_id)
}

async fn facility(
    repository: &PgFacilityRepository,
    workspace_id: WorkspaceId,
    name: &str,
    role: FacilityRole,
) -> IndustryFacilityProfile {
    let command = CreateFacilityProfileCommand {
        name: name.into(),
        kind: FacilityKind::Manual,
        role,
        structure_id: None,
        structure_type_id: None,
        structure_type_name: String::new(),
        solar_system_id: None,
        solar_system_name: "Jita".into(),
        security_class: SecurityClass::HighSec,
        material_reduction_percent: "1".into(),
        time_reduction_percent: "15".into(),
        job_cost_reduction_percent: "0".into(),
        facility_tax_percent: "0".into(),
        scc_surcharge_percent: "0".into(),
        alliance_surcharge_percent: "0".into(),
        fixed_supplemental_cost: String::new(),
        manual_system_cost_index: Some("0.05".into()),
        notes: String::new(),
        rigs: vec![],
    };
    repository
        .create(iskworks_core::parse_profile(workspace_id, command).unwrap())
        .await
        .unwrap()
}

fn set(profile: &IndustryFacilityProfile, target: Option<i64>) -> FacilityDefaultChange {
    FacilityDefaultChange::Set(FacilityDefault {
        role: profile.role,
        target_filter_id: target,
        facility_profile_id: profile.id,
        rig_covered_at_assignment: target.is_some(),
    })
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn set_upserts_per_target_and_clear_removes_only_that_target(pool: PgPool) {
    let workspace_id = workspace(&pool).await;
    let repository = PgFacilityRepository::new(pool.clone());
    let a = facility(&repository, workspace_id, "A", FacilityRole::Manufacturing).await;
    let b = facility(&repository, workspace_id, "B", FacilityRole::Manufacturing).await;

    repository
        .apply_facility_default_changes(workspace_id, vec![set(&a, Some(5)), set(&a, None)])
        .await
        .unwrap();
    repository
        .apply_facility_default_changes(workspace_id, vec![set(&b, Some(5))])
        .await
        .unwrap();
    let defaults = repository
        .list_facility_defaults(workspace_id)
        .await
        .unwrap();
    assert_eq!(defaults.len(), 2);
    assert_eq!(defaults[0].target_filter_id, Some(5));
    assert_eq!(defaults[0].facility_profile_id, b.id);
    assert_eq!(defaults[1].target_filter_id, None);
    assert!(!defaults[1].rig_covered_at_assignment);

    repository
        .apply_facility_default_changes(
            workspace_id,
            vec![FacilityDefaultChange::Clear {
                role: FacilityRole::Manufacturing,
                target_filter_id: None,
            }],
        )
        .await
        .unwrap();
    let defaults = repository
        .list_facility_defaults(workspace_id)
        .await
        .unwrap();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].target_filter_id, Some(5));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn set_rejects_a_facility_of_another_role_or_workspace_atomically(pool: PgPool) {
    let workspace_id = workspace(&pool).await;
    let other_workspace = workspace(&pool).await;
    let repository = PgFacilityRepository::new(pool.clone());
    let manufacturing = facility(&repository, workspace_id, "M", FacilityRole::Manufacturing).await;
    let reaction = facility(&repository, workspace_id, "R", FacilityRole::Reaction).await;
    let foreign = facility(
        &repository,
        other_workspace,
        "F",
        FacilityRole::Manufacturing,
    )
    .await;

    let mut wrong_role = set(&reaction, Some(16));
    if let FacilityDefaultChange::Set(default) = &mut wrong_role {
        default.role = FacilityRole::Manufacturing;
    }
    let error = repository
        .apply_facility_default_changes(workspace_id, vec![set(&manufacturing, None), wrong_role])
        .await
        .unwrap_err();
    assert!(matches!(error, FacilityError::Validation(_)));
    let error = repository
        .apply_facility_default_changes(workspace_id, vec![set(&foreign, None)])
        .await
        .unwrap_err();
    assert!(matches!(error, FacilityError::Validation(_)));
    assert!(repository
        .list_facility_defaults(workspace_id)
        .await
        .unwrap()
        .is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn archiving_or_deleting_a_facility_drops_its_defaults(pool: PgPool) {
    let workspace_id = workspace(&pool).await;
    let repository = PgFacilityRepository::new(pool.clone());
    let archived = facility(
        &repository,
        workspace_id,
        "Archived",
        FacilityRole::Manufacturing,
    )
    .await;
    let deleted = facility(&repository, workspace_id, "Deleted", FacilityRole::Reaction).await;
    let kept = facility(
        &repository,
        workspace_id,
        "Kept",
        FacilityRole::Manufacturing,
    )
    .await;
    repository
        .apply_facility_default_changes(
            workspace_id,
            vec![
                set(&archived, Some(5)),
                set(&deleted, Some(16)),
                set(&kept, None),
            ],
        )
        .await
        .unwrap();

    let archived_now = repository
        .archive(workspace_id, archived.id, archived.revision)
        .await
        .unwrap();
    assert!(archived_now.archived_at.is_some());
    repository
        .delete(workspace_id, deleted.id, deleted.revision)
        .await
        .unwrap();

    let defaults = repository
        .list_facility_defaults(workspace_id)
        .await
        .unwrap();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].facility_profile_id, kept.id);

    let error = repository
        .apply_facility_default_changes(workspace_id, vec![set(&archived_now, Some(5))])
        .await
        .unwrap_err();
    assert!(matches!(error, FacilityError::Validation(_)));
}
