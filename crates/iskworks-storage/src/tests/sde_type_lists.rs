use super::*;
use iskworks_sde::{
    CandidateRecipeKind, ImportDogmaAttribute, ImportTypeAttribute, ImportTypeDetails,
    TypeListEntry, TypeUsage, UsedWithKind,
};
use rust_decimal::Decimal;

const GD_I: i64 = 37_543;
const GD_COMPACT: i64 = 5_321;
const GD_II: i64 = 37_546;
const GD_STORYLINE: i64 = 22_931;
const GD_RETIRED: i64 = 37_999;
const SCRIPT: i64 = 40_334;
const LARGE_SCRIPT: i64 = 40_999;
const LARGE_DISRUPTOR: i64 = 41_000;
const DISRUPTOR_GROUP: i64 = 291;
const SCRIPT_GROUP: i64 = 1_569;

const CHARGE_SIZE: i64 = 128;
const CHARGE_GROUP_1: i64 = 604;
const META_LEVEL: i64 = 633;

async fn import_active(repository: &PgSdeRepository, dataset: &NormalizedSde) {
    let import_id = repository
        .begin_import(NewImport {
            source_version: dataset.source_version.clone(),
            source_label: dataset.source_label.clone(),
            source_checksum: dataset.source_checksum.clone(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    repository
        .write_dataset(import_id, dataset, &NoopProgressReporter)
        .await
        .unwrap();
    repository
        .activate_import(import_id, dataset.counts(), Utc::now())
        .await
        .unwrap();
}

fn item(type_id: i64, name: &str, group_id: i64, meta_group_id: Option<i64>) -> ImportType {
    ImportType {
        type_id,
        name: name.to_string(),
        group_id: Some(group_id),
        group_name: None,
        market_group_id: None,
        meta_group_id,
        packaged_volume_m3: None,
        published: true,
    }
}

fn attribute(attribute_id: i64, name: &str) -> ImportDogmaAttribute {
    ImportDogmaAttribute {
        attribute_id,
        name: name.to_string(),
        display_name: name.to_string(),
        unit_id: None,
        category_id: None,
        category_name: None,
        display_when_zero: false,
        high_is_good: true,
    }
}

fn value(type_id: i64, attribute_id: i64, value: i64) -> ImportTypeAttribute {
    ImportTypeAttribute {
        type_id,
        attribute_id,
        value: Decimal::from(value),
    }
}

fn child_of(type_id: i64, parent: i64) -> ImportTypeDetails {
    ImportTypeDetails {
        type_id,
        description: None,
        volume_m3: None,
        mass_kg: None,
        capacity_m3: None,
        portion_size: None,
        variation_parent_type_id: Some(parent),
    }
}

/// The Guidance Disruptor family (one retired, unpublished member), the
/// script it loads, and a large script only a large disruptor loads.
fn lists_fixture(checksum: &str) -> NormalizedSde {
    let mut dataset = fixture_sde(checksum, "100005");
    dataset.meta_groups.extend([
        ImportMetaGroup {
            meta_group_id: 2,
            name: "Tech II".into(),
        },
        ImportMetaGroup {
            meta_group_id: 3,
            name: "Storyline".into(),
        },
    ]);
    dataset.groups.extend([
        ImportGroup {
            group_id: DISRUPTOR_GROUP,
            name: "Weapon Disruptor".into(),
            category_id: Some(6),
            published: true,
        },
        ImportGroup {
            group_id: SCRIPT_GROUP,
            name: "Guidance Disruption Script".into(),
            category_id: Some(6),
            published: true,
        },
    ]);
    let retired = ImportType {
        published: false,
        ..item(
            GD_RETIRED,
            "Old Guidance Disruptor",
            DISRUPTOR_GROUP,
            Some(1),
        )
    };
    dataset.types.extend([
        item(GD_I, "Guidance Disruptor I", DISRUPTOR_GROUP, Some(1)),
        item(
            GD_COMPACT,
            "C-IR Compact Guidance Disruptor",
            DISRUPTOR_GROUP,
            Some(1),
        ),
        item(GD_II, "Guidance Disruptor II", DISRUPTOR_GROUP, Some(2)),
        item(
            GD_STORYLINE,
            "'Distributor' Guidance Disruptor",
            DISRUPTOR_GROUP,
            Some(3),
        ),
        retired,
        item(
            LARGE_DISRUPTOR,
            "Large Guidance Disruptor",
            DISRUPTOR_GROUP,
            None,
        ),
        // Scripts carry no meta group in the SDE.
        item(
            SCRIPT,
            "Missile Range Disruption Script",
            SCRIPT_GROUP,
            None,
        ),
        item(
            LARGE_SCRIPT,
            "Large Missile Range Disruption Script",
            SCRIPT_GROUP,
            None,
        ),
    ]);
    dataset.type_info.details = vec![
        child_of(GD_COMPACT, GD_I),
        child_of(GD_II, GD_I),
        child_of(GD_STORYLINE, GD_I),
        child_of(GD_RETIRED, GD_I),
    ];
    dataset.type_info.dogma_attributes = vec![
        attribute(CHARGE_SIZE, "chargeSize"),
        attribute(CHARGE_GROUP_1, "chargeGroup1"),
        attribute(META_LEVEL, "metaLevelOld"),
    ];
    dataset.type_info.type_attributes = vec![
        value(GD_COMPACT, META_LEVEL, 1),
        value(GD_I, META_LEVEL, 0),
        value(GD_II, META_LEVEL, 5),
        value(GD_STORYLINE, META_LEVEL, 6),
        value(GD_I, CHARGE_GROUP_1, SCRIPT_GROUP),
        value(GD_COMPACT, CHARGE_GROUP_1, SCRIPT_GROUP),
        value(GD_II, CHARGE_GROUP_1, SCRIPT_GROUP),
        value(GD_STORYLINE, CHARGE_GROUP_1, SCRIPT_GROUP),
        value(GD_RETIRED, CHARGE_GROUP_1, SCRIPT_GROUP),
        // Only the large disruptor loads the large (size 3) script; the
        // small modules carry size 1.
        value(LARGE_DISRUPTOR, CHARGE_GROUP_1, SCRIPT_GROUP),
        value(LARGE_DISRUPTOR, CHARGE_SIZE, 3),
        value(GD_I, CHARGE_SIZE, 1),
        value(GD_COMPACT, CHARGE_SIZE, 1),
        value(GD_II, CHARGE_SIZE, 1),
        value(GD_STORYLINE, CHARGE_SIZE, 1),
        value(SCRIPT, CHARGE_SIZE, 1),
        value(LARGE_SCRIPT, CHARGE_SIZE, 3),
    ];
    dataset
}

fn entry(type_id: i64, name: &str, meta: (i64, &str), meta_level: Option<i64>) -> TypeListEntry {
    TypeListEntry {
        type_id,
        name: name.to_string(),
        meta_group_id: meta.0,
        meta_group_name: meta.1.to_string(),
        meta_level,
    }
}

const TECH_I: (i64, &str) = (1, "Tech I");
const TECH_II: (i64, &str) = (2, "Tech II");
const STORYLINE: (i64, &str) = (3, "Storyline");

fn family() -> Vec<TypeListEntry> {
    vec![
        entry(GD_I, "Guidance Disruptor I", TECH_I, Some(0)),
        entry(
            GD_COMPACT,
            "C-IR Compact Guidance Disruptor",
            TECH_I,
            Some(1),
        ),
        entry(GD_II, "Guidance Disruptor II", TECH_II, Some(5)),
        entry(
            GD_STORYLINE,
            "'Distributor' Guidance Disruptor",
            STORYLINE,
            Some(6),
        ),
    ]
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn variations_are_the_published_family_from_any_member(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    import_active(&repository, &lists_fixture("checksum-lists-variations")).await;

    for member in [GD_I, GD_II, GD_STORYLINE] {
        assert_eq!(
            repository.type_variations(member).await.unwrap(),
            family(),
            "from {member}"
        );
    }
    // No family: just itself, Tech I when it has no meta group.
    assert_eq!(
        repository.type_variations(SCRIPT).await.unwrap(),
        [entry(
            SCRIPT,
            "Missile Range Disruption Script",
            TECH_I,
            None
        )]
    );
    assert!(repository
        .type_variations(999_999)
        .await
        .unwrap()
        .is_empty());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn used_with_goes_from_a_charge_to_its_modules_and_back(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    import_active(&repository, &lists_fixture("checksum-lists-used-with")).await;

    // The small script fits the small disruptors, not the large one, and
    // never the unpublished one.
    assert_eq!(
        repository.types_used_with(SCRIPT).await.unwrap(),
        Some((UsedWithKind::Modules, family()))
    );
    assert_eq!(
        repository.types_used_with(LARGE_SCRIPT).await.unwrap(),
        Some((
            UsedWithKind::Modules,
            vec![entry(
                LARGE_DISRUPTOR,
                "Large Guidance Disruptor",
                TECH_I,
                None
            )],
        ))
    );
    assert_eq!(
        repository.types_used_with(GD_II).await.unwrap(),
        Some((
            UsedWithKind::Charges,
            vec![entry(
                SCRIPT,
                "Missile Range Disruption Script",
                TECH_I,
                None
            )],
        ))
    );
    // Neither a module with charge groups nor a charge anything loads.
    assert_eq!(repository.types_used_with(5_876).await.unwrap(), None);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn usages_page_through_the_recipes_that_consume_a_type(pool: PgPool) {
    let repository = PgSdeRepository::new(pool);
    import_active(&repository, &lists_fixture("checksum-lists-usages")).await;

    let blueprint = TypeUsage {
        kind: CandidateRecipeKind::Manufacturing,
        recipe_type_id: 6_830,
        recipe_name: "Rifter Blueprint".into(),
        product_type_id: 5_876,
        product_name: "Rifter".into(),
        quantity: 1_000,
    };
    let reaction = TypeUsage {
        kind: CandidateRecipeKind::Reaction,
        recipe_type_id: 46_157,
        recipe_name: "Methanofullerene Reaction Formula".into(),
        product_type_id: 5_876,
        product_name: "Rifter".into(),
        quantity: 100,
    };
    assert_eq!(
        repository.type_usages(34, 50, 0).await.unwrap(),
        (vec![blueprint.clone(), reaction.clone()], 2)
    );
    assert_eq!(
        repository.type_usages(34, 1, 0).await.unwrap(),
        (vec![blueprint], 2)
    );
    assert_eq!(
        repository.type_usages(34, 1, 1).await.unwrap(),
        (vec![reaction], 2)
    );
    assert_eq!(
        repository.type_usages(5_876, 50, 0).await.unwrap(),
        (Vec::new(), 0)
    );
}
