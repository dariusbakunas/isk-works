use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::*;
use crate::{BlueprintKind, BlueprintObservation, OwnerId, WorkspaceId};

fn observation(
    id: u128,
    kind: BlueprintKind,
    me: u8,
    te: u8,
    licensed_runs: Option<u64>,
) -> BlueprintObservation {
    BlueprintObservation {
        id: Uuid::from_u128(id),
        workspace_id: WorkspaceId(Uuid::from_u128(1)),
        owner_id: OwnerId(Uuid::from_u128(2)),
        owner_name: "Owner".into(),
        eve_item_id: i64::try_from(id).unwrap(),
        blueprint_type_id: 5_000,
        blueprint_name: "Capital Armor Plates Blueprint".into(),
        kind,
        material_efficiency: me,
        time_efficiency: te,
        licensed_runs,
        location_id: 60_003_760,
        location_flag: "Hangar".into(),
        location_name: Some("Jita IV - Moon 4".into()),
        observed_at: Utc
            .with_ymd_and_hms(2026, 11, 1, 0, 0, u32::try_from(id).unwrap())
            .unwrap(),
        imported_at: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
    }
}

fn producer(required_runs: u64, source: ProducerBlueprintSource) -> ProducerBlueprintUse {
    ProducerBlueprintUse {
        required_runs,
        source,
        observation_id: None,
        material_efficiency: Some(10),
        time_efficiency: Some(20),
    }
}

fn state(
    sourcing: WorksheetSourcing,
    observations: &[BlueprintObservation],
    required_runs: Option<u64>,
) -> BlueprintCoverageState {
    let holdings = summarize_holdings(observations);
    let producer = required_runs.map(|runs| producer(runs, ProducerBlueprintSource::Manual));
    coverage_state(sourcing, &holdings, producer.as_ref())
}

#[test]
fn an_original_is_owned_on_buy_and_manufacturing_rows() {
    let original = [observation(1, BlueprintKind::Original, 10, 20, None)];
    assert_eq!(
        state(WorksheetSourcing::Buy, &original, None),
        BlueprintCoverageState::Owned
    );
    assert_eq!(
        state(WorksheetSourcing::Manufacturing, &original, Some(500)),
        BlueprintCoverageState::Owned
    );
}

#[test]
fn copies_on_a_buy_row_are_owned_without_a_run_comparison() {
    let copy = [observation(1, BlueprintKind::Copy, 10, 20, Some(1))];
    assert_eq!(
        state(WorksheetSourcing::Buy, &copy, None),
        BlueprintCoverageState::Owned
    );
}

#[test]
fn copies_cover_a_producer_by_total_licensed_runs() {
    let copies = [
        observation(1, BlueprintKind::Copy, 10, 20, Some(4)),
        observation(2, BlueprintKind::Copy, 10, 20, Some(4)),
        observation(3, BlueprintKind::Copy, 10, 20, Some(4)),
    ];
    assert_eq!(
        state(WorksheetSourcing::Manufacturing, &copies, Some(12)),
        BlueprintCoverageState::Owned
    );
    assert_eq!(
        state(WorksheetSourcing::Manufacturing, &copies, Some(13)),
        BlueprintCoverageState::InsufficientRuns
    );
}

#[test]
fn a_copy_without_known_runs_adds_no_runs() {
    let copies = [
        observation(1, BlueprintKind::Copy, 10, 20, None),
        observation(2, BlueprintKind::Copy, 10, 20, Some(0)),
    ];
    let holdings = summarize_holdings(&copies);
    assert_eq!(holdings.copies, 2);
    assert_eq!(holdings.copy_runs, 0);
    assert_eq!(
        state(WorksheetSourcing::Manufacturing, &copies, Some(1)),
        BlueprintCoverageState::InsufficientRuns
    );
}

#[test]
fn only_unknown_kind_blueprints_report_unknown_kind() {
    let unknown = [observation(1, BlueprintKind::Unknown, 0, 0, Some(1))];
    assert_eq!(
        state(WorksheetSourcing::Manufacturing, &unknown, Some(5)),
        BlueprintCoverageState::UnknownKind
    );
    assert_eq!(
        state(WorksheetSourcing::Buy, &unknown, None),
        BlueprintCoverageState::UnknownKind
    );
}

#[test]
fn nothing_owned_is_not_owned() {
    assert_eq!(
        state(WorksheetSourcing::Buy, &[], None),
        BlueprintCoverageState::NotOwned
    );
    assert_eq!(
        state(WorksheetSourcing::Manufacturing, &[], Some(1)),
        BlueprintCoverageState::NotOwned
    );
}

#[test]
fn best_prefers_me_then_te_then_original() {
    let observations = [
        observation(1, BlueprintKind::Copy, 9, 20, Some(10)),
        observation(2, BlueprintKind::Copy, 10, 18, Some(10)),
        observation(3, BlueprintKind::Copy, 10, 20, Some(10)),
        observation(4, BlueprintKind::Original, 10, 20, None),
    ];
    let holdings = summarize_holdings(&observations);
    let best = holdings.best.unwrap();
    assert_eq!(best.observation_id, Uuid::from_u128(4));
    assert_eq!(best.kind, BlueprintKind::Original);
    assert_eq!(holdings.originals, 1);
    assert_eq!(holdings.copies, 3);
    assert_eq!(holdings.copy_runs, 30);
    assert_eq!(holdings.observed_at, Some(observations[3].observed_at));
}

#[test]
fn producer_source_follows_the_persisted_selection() {
    let observed = BlueprintSelection::ObservedAsset {
        observation_id: Uuid::from_u128(9),
        kind: BlueprintKind::Copy,
        material_efficiency: 10,
        time_efficiency: 20,
        licensed_runs: Some(5),
    };
    assert_eq!(
        ProducerBlueprintSource::from_selection(Some(&observed)),
        (
            ProducerBlueprintSource::ObservedAsset,
            Some(Uuid::from_u128(9))
        )
    );
    assert_eq!(
        ProducerBlueprintSource::from_selection(None),
        (ProducerBlueprintSource::Manual, None)
    );
}
