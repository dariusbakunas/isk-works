use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::TimeZone;
use iskworks_core::{ConnectedCharacterId, InventoryError};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, ProductRecipeRefs, RecipeLine,
    RequiredSkill, SdeError, SkillAttribute, SkillDefinition, TypeAttributeValue, TypeBonusLine,
    TypeBonusSource, TypeInfoBase, TypeInfoCounts, TypeListEntry, TypeSearchResult, TypeUsage,
    UsedWithKind,
};
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::SkillsCharacter;

const HARPY: i64 = 11_381;
const SCRIPT: i64 = 40_334;
const HARPY_BLUEPRINT: i64 = 11_382;
const TRITANIUM: i64 = 34;
const SPACESHIP_COMMAND: i64 = 3_327;
const CALDARI_FRIGATE: i64 = 3_330;
const ASSAULT_FRIGATES: i64 = 12_095;
const MISSILE_GUIDANCE_COMPUTER: i64 = 1_396;

/// The Harpy and the Missile Range Disruption Script, as PR 1 stores them.
struct FakeSde;

fn harpy() -> TypeInfoBase {
    TypeInfoBase {
        type_id: HARPY,
        name: "Harpy".into(),
        published: true,
        group_id: Some(324),
        group_name: Some("Assault Frigate".into()),
        category_id: Some(6),
        category_name: Some("Ship".into()),
        meta_group_id: Some(2),
        meta_group_name: Some("Tech II".into()),
        market_group_path: vec!["Ships".into(), "Frigates".into(), "Assault Frigates".into()],
        packaged_volume_m3: Some(Decimal::new(2_500, 0)),
        description: Some("A <a href=showinfo:3330>Caldari</a> railgun platform.".into()),
        volume_m3: Some(Decimal::new(28_600, 0)),
        mass_kg: Some(Decimal::new(1_155_000, 0)),
        capacity_m3: Some(Decimal::new(165, 0)),
        portion_size: Some(1),
        variation_parent_type_id: Some(603),
    }
}

fn script() -> TypeInfoBase {
    TypeInfoBase {
        type_id: SCRIPT,
        name: "Missile Range Disruption Script".into(),
        published: true,
        group_id: Some(1_400),
        group_name: Some("Guidance Disruption Script".into()),
        category_id: Some(8),
        category_name: Some("Charge".into()),
        meta_group_id: Some(1),
        meta_group_name: Some("Tech I".into()),
        volume_m3: Some(Decimal::ONE),
        packaged_volume_m3: Some(Decimal::ONE),
        mass_kg: Some(Decimal::ONE),
        ..TypeInfoBase::default()
    }
}

/// Not the real Harpy family; enough for grouping.
fn harpy_family() -> Vec<TypeListEntry> {
    let entry = |type_id, name: &str, meta: (i64, &str), meta_level| TypeListEntry {
        type_id,
        name: name.into(),
        meta_group_id: meta.0,
        meta_group_name: meta.1.into(),
        meta_level,
    };
    vec![
        entry(603, "Merlin", (1, "Tech I"), Some(0)),
        entry(HARPY, "Harpy", (2, "Tech II"), Some(5)),
        entry(11_379, "Hawk", (2, "Tech II"), Some(5)),
        entry(17_619, "Caldari Navy Hookbill", (4, "Faction"), None),
    ]
}

fn attribute(attribute_id: i64, label: &str, unit: (i64, &str), value: i64) -> TypeAttributeValue {
    TypeAttributeValue {
        attribute_id,
        display_name: label.into(),
        unit_id: Some(unit.0),
        unit_display_name: Some(unit.1.into()),
        category_id: Some(7),
        category_name: Some("Miscellaneous".into()),
        display_when_zero: false,
        high_is_good: true,
        value: Decimal::from(value),
    }
}

#[async_trait]
impl SdeReadRepository for FakeSde {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(None)
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
    async fn type_info_base(&self, type_id: i64) -> Result<Option<TypeInfoBase>, SdeError> {
        Ok(match type_id {
            HARPY => Some(harpy()),
            SCRIPT => Some(script()),
            _ => None,
        })
    }
    async fn type_attributes(&self, type_id: i64) -> Result<Vec<TypeAttributeValue>, SdeError> {
        Ok(match type_id {
            SCRIPT => vec![
                attribute(128, "Charge size", (117, "1=small 2=medium 3=l"), 1),
                attribute(137, "Used with (Launcher Group)", (115, "groupID"), 1_396),
                attribute(
                    2023,
                    "Modification of Explosion Radius Bonus",
                    (105, "%"),
                    -100,
                ),
            ],
            _ => Vec::new(),
        })
    }
    async fn type_bonuses(&self, type_id: i64) -> Result<Vec<TypeBonusLine>, SdeError> {
        let percent = |source, bonus: i64, text: &str| TypeBonusLine {
            source,
            bonus: Some(Decimal::from(bonus)),
            unit_id: Some(105),
            unit_display_name: Some("%".into()),
            text: text.into(),
        };
        Ok(match type_id {
            HARPY => vec![
                percent(
                    TypeBonusSource::Skill(CALDARI_FRIGATE),
                    4,
                    "bonus to all shield resistances",
                ),
                percent(
                    TypeBonusSource::Skill(ASSAULT_FRIGATES),
                    5,
                    "Small Hybrid Turret damage",
                ),
                TypeBonusLine {
                    source: TypeBonusSource::Role,
                    bonus: None,
                    unit_id: None,
                    unit_display_name: None,
                    text: "Can fit <a href=showinfo:47254>Assault Damage Controls</a>".into(),
                },
            ],
            _ => Vec::new(),
        })
    }
    async fn type_materials(&self, type_id: i64) -> Result<Vec<RecipeLine>, SdeError> {
        Ok(match type_id {
            HARPY => vec![RecipeLine {
                type_id: TRITANIUM,
                type_name: "Tritanium".into(),
                quantity: 14_000,
            }],
            _ => Vec::new(),
        })
    }
    async fn type_info_counts(&self, type_id: i64) -> Result<TypeInfoCounts, SdeError> {
        Ok(match type_id {
            HARPY => TypeInfoCounts {
                variations: 4,
                used_in: 0,
            },
            _ => TypeInfoCounts {
                variations: 1,
                used_in: 3,
            },
        })
    }
    async fn type_variations(&self, type_id: i64) -> Result<Vec<TypeListEntry>, SdeError> {
        Ok(if type_id == HARPY {
            harpy_family()
        } else {
            Vec::new()
        })
    }
    async fn types_used_with(
        &self,
        type_id: i64,
    ) -> Result<Option<(UsedWithKind, Vec<TypeListEntry>)>, SdeError> {
        Ok((type_id == SCRIPT).then(|| (UsedWithKind::Modules, harpy_family())))
    }
    async fn type_usages(
        &self,
        type_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<TypeUsage>, i64), SdeError> {
        let all: Vec<TypeUsage> = if type_id == TRITANIUM {
            (0..3)
                .map(|index| TypeUsage {
                    kind: CandidateRecipeKind::Manufacturing,
                    recipe_type_id: 100 + index,
                    recipe_name: format!("Thing {index} Blueprint"),
                    product_type_id: 200 + index,
                    product_name: format!("Thing {index}"),
                    quantity: 10 * (index + 1),
                })
                .collect()
        } else {
            Vec::new()
        };
        let total = all.len() as i64;
        Ok((
            all.into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect(),
            total,
        ))
    }
    async fn production_recipes_for_products(
        &self,
        product_type_ids: &[i64],
    ) -> Result<BTreeMap<i64, ProductRecipeRefs>, SdeError> {
        Ok(product_type_ids
            .iter()
            .filter(|id| **id == HARPY)
            .map(|id| {
                (
                    *id,
                    ProductRecipeRefs {
                        blueprint_type_id: Some(HARPY_BLUEPRINT),
                        reaction_formula_type_id: None,
                    },
                )
            })
            .collect())
    }
    async fn type_names(&self, type_ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        let known = [
            (HARPY_BLUEPRINT, "Harpy Blueprint"),
            (CALDARI_FRIGATE, "Caldari Frigate"),
            (ASSAULT_FRIGATES, "Assault Frigates"),
        ];
        Ok(known
            .into_iter()
            .filter(|(id, _)| type_ids.contains(id))
            .map(|(id, name)| (id, name.to_string()))
            .collect())
    }
    async fn group_names(&self, group_ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        Ok(group_ids
            .iter()
            .filter(|id| **id == MISSILE_GUIDANCE_COMPUTER)
            .map(|id| (*id, "Missile Guidance Computer".to_string()))
            .collect())
    }
    async fn required_skills(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<RequiredSkill>>, SdeError> {
        let needs = |skill_type_id, level| RequiredSkill {
            skill_type_id,
            level,
        };
        Ok(type_ids
            .iter()
            .filter(|id| **id == HARPY)
            .map(|id| {
                (
                    *id,
                    vec![needs(CALDARI_FRIGATE, 5), needs(ASSAULT_FRIGATES, 1)],
                )
            })
            .collect())
    }
    async fn skill_catalogue(&self) -> Result<Vec<SkillDefinition>, SdeError> {
        let skill = |type_id, name: &str, prerequisites: Vec<(i64, i16)>| SkillDefinition {
            type_id,
            name: name.into(),
            group_id: 257,
            group_name: "Spaceship Command".into(),
            rank: 1,
            primary_attribute: SkillAttribute::Perception,
            secondary_attribute: SkillAttribute::Willpower,
            omega_only: false,
            prerequisites: prerequisites
                .into_iter()
                .map(|(skill_type_id, level)| RequiredSkill {
                    skill_type_id,
                    level,
                })
                .collect(),
        };
        Ok(vec![
            skill(SPACESHIP_COMMAND, "Spaceship Command", Vec::new()),
            skill(
                CALDARI_FRIGATE,
                "Caldari Frigate",
                vec![(SPACESHIP_COMMAND, 1)],
            ),
            skill(
                ASSAULT_FRIGATES,
                "Assault Frigates",
                vec![(SPACESHIP_COMMAND, 3)],
            ),
        ])
    }
}

struct FakeCharacters(Vec<SkillsCharacter>);

#[async_trait]
impl SkillsCharacterSource for FakeCharacters {
    async fn skills_character(
        &self,
        _: WorkspaceId,
        connection_id: ConnectedCharacterId,
    ) -> Result<Option<SkillsCharacter>, InventoryError> {
        Ok(self
            .0
            .iter()
            .find(|character| character.connection_id == connection_id)
            .cloned())
    }
    async fn skills_characters(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<SkillsCharacter>, InventoryError> {
        Ok(self.0.clone())
    }
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap()
}

/// Spaceship Command III and Caldari Frigate IV: short of the Harpy.
fn aura() -> SkillsCharacter {
    SkillsCharacter {
        connection_id: ConnectedCharacterId(Uuid::from_u128(7)),
        name: "Aura Starfall".into(),
        skills_scope_granted: true,
        skills_summary: Some(json!({
            "total_sp": 1_000_000,
            "skills": [
                { "skill_id": SPACESHIP_COMMAND, "active_skill_level": 3,
                  "trained_skill_level": 3, "skillpoints_in_skill": 8_000 },
                { "skill_id": CALDARI_FRIGATE, "active_skill_level": 4,
                  "trained_skill_level": 4, "skillpoints_in_skill": 45_255 }
            ],
            "attributes": { "charisma": 20, "intelligence": 20, "memory": 20,
                            "perception": 20, "willpower": 20 }
        })),
        observed_at: Some(now()),
    }
}

fn service() -> TypeInfoService {
    TypeInfoService::new(Arc::new(FakeSde)).with_characters(Arc::new(FakeCharacters(vec![aura()])))
}

async fn info(type_id: i64, pilot: PilotChoice) -> Result<Option<TypeInfoView>, InventoryError> {
    service()
        .info(WorkspaceId::new(), type_id, pilot, None, now())
        .await
}

#[tokio::test]
async fn the_harpy_has_its_header_traits_requirements_and_industry() {
    let view = info(HARPY, PilotChoice::AllV).await.unwrap().unwrap();

    assert_eq!(view.name, "Harpy");
    assert_eq!(
        view.group,
        Some(NamedId {
            id: 324,
            name: "Assault Frigate".into(),
        })
    );
    assert_eq!(
        view.category.map(|category| category.name),
        Some("Ship".into())
    );
    assert_eq!(
        view.meta_group.map(|meta| meta.name),
        Some("Tech II".into())
    );
    assert_eq!(view.market_group_path.len(), 3);
    assert_eq!(
        view.description,
        [
            RichSegment::Text {
                text: "A ".into(),
                bold: false,
            },
            RichSegment::Link {
                type_id: CALDARI_FRIGATE,
                text: "Caldari".into(),
            },
            RichSegment::Text {
                text: " railgun platform.".into(),
                bold: false,
            },
        ]
    );
    assert_eq!(
        view.basics,
        TypeBasics {
            volume_m3: Some(Decimal::new(28_600, 0)),
            packaged_volume_m3: Some(Decimal::new(2_500, 0)),
            mass_kg: Some(Decimal::new(1_155_000, 0)),
            capacity_m3: Some(Decimal::new(165, 0)),
            portion_size: Some(1),
        }
    );
    let headings: Vec<&str> = view
        .traits
        .iter()
        .map(|group| group.heading.as_str())
        .collect();
    assert_eq!(
        headings,
        [
            "Caldari Frigate bonuses (per skill level)",
            "Assault Frigates bonuses (per skill level)",
            "Role Bonus",
        ]
    );
    let requirements: Vec<(i64, i16, usize)> = view
        .requirements
        .iter()
        .map(|node| (node.skill_type_id, node.level, node.children.len()))
        .collect();
    assert_eq!(
        requirements,
        [(CALDARI_FRIGATE, 5, 1), (ASSAULT_FRIGATES, 1, 1)]
    );
    assert!(view.requirements.iter().all(SkillNode::met));
    assert_eq!(view.missing, None, "All V has nothing to train");
    assert_eq!(
        view.industry,
        TypeIndustry {
            produced_by: vec![TypeProducer {
                kind: CandidateRecipeKind::Manufacturing,
                type_id: HARPY_BLUEPRINT,
                name: Some("Harpy Blueprint".into()),
            }],
            reprocesses_into: vec![TypeQuantity {
                type_id: TRITANIUM,
                name: "Tritanium".into(),
                quantity: 14_000,
            }],
            used_in_count: 0,
        }
    );
    assert_eq!(view.variation_count, 4);
    assert_eq!(view.price, None, "no market configured");
}

#[tokio::test]
async fn the_scripts_attributes_read_like_the_client() {
    let view = info(SCRIPT, PilotChoice::AllV).await.unwrap().unwrap();

    assert_eq!(view.attributes.len(), 1);
    let rows: Vec<(&str, &str)> = view.attributes[0]
        .rows
        .iter()
        .map(|row| (row.label.as_str(), row.display.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Charge size", "Small"),
            ("Used with (Launcher Group)", "Missile Guidance Computer"),
            ("Modification of Explosion Radius Bonus", "-100 %"),
        ]
    );
    assert!(view.traits.is_empty());
    assert!(view.requirements.is_empty());
    assert!(view.industry.produced_by.is_empty());
    assert_eq!(view.industry.used_in_count, 3);
    assert!(view.description.is_empty());
}

#[tokio::test]
async fn a_character_pilot_gets_its_levels_and_what_it_lacks() {
    let view = info(HARPY, PilotChoice::Character(aura().connection_id))
        .await
        .unwrap()
        .unwrap();

    let levels: Vec<(i64, i16, i16)> = view
        .requirements
        .iter()
        .flat_map(|node| std::iter::once(node).chain(&node.children))
        .map(|node| (node.skill_type_id, node.level, node.trained_level))
        .collect();
    assert_eq!(
        levels,
        [
            (CALDARI_FRIGATE, 5, 4),
            (SPACESHIP_COMMAND, 1, 3),
            (ASSAULT_FRIGATES, 1, 0),
            (SPACESHIP_COMMAND, 3, 3),
        ]
    );
    let missing = view.missing.expect("a character's summary");
    assert_eq!(missing.missing_skills, 2);
    assert!(missing.remaining_seconds.is_some_and(|seconds| seconds > 0));
}

#[tokio::test]
async fn all_zero_checks_without_a_summary() {
    let view = info(HARPY, PilotChoice::AllZero).await.unwrap().unwrap();
    assert!(view.requirements.iter().all(|node| node.trained_level == 0));
    assert_eq!(view.missing, None);
}

#[tokio::test]
async fn an_unknown_character_is_a_validation_error() {
    let error = info(
        HARPY,
        PilotChoice::Character(ConnectedCharacterId(Uuid::from_u128(99))),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, InventoryError::Validation(_)), "{error:?}");
}

#[tokio::test]
async fn a_type_the_import_lacks_has_no_info() {
    assert_eq!(info(77_777, PilotChoice::AllV).await.unwrap(), None);
}

#[tokio::test]
async fn info_says_whether_there_is_a_used_with_list() {
    let script_view = info(SCRIPT, PilotChoice::AllV).await.unwrap().unwrap();
    assert_eq!(
        script_view.used_with,
        Some(UsedWithSummary {
            kind: UsedWithKind::Modules,
            count: 4,
        })
    );
    let harpy_view = info(HARPY, PilotChoice::AllV).await.unwrap().unwrap();
    assert_eq!(harpy_view.used_with, None);
}

#[tokio::test]
async fn variations_are_grouped_by_meta_group() {
    let groups = service().variations(HARPY).await.unwrap();

    let layout: Vec<(&str, Vec<&str>)> = groups
        .iter()
        .map(|group| {
            (
                group.meta_group_name.as_str(),
                group.items.iter().map(|item| item.name.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        layout,
        [
            ("Tech I", vec!["Merlin"]),
            ("Tech II", vec!["Harpy", "Hawk"]),
            ("Faction", vec!["Caldari Navy Hookbill"]),
        ]
    );
    assert_eq!(groups[1].meta_group_id, 2);
    assert_eq!(groups[1].items[0].meta_level, Some(5));
    assert!(service().variations(SCRIPT).await.unwrap().is_empty());
}

#[tokio::test]
async fn used_with_is_grouped_too_and_empty_for_neither() {
    let view = service().used_with(SCRIPT).await.unwrap();
    assert_eq!(view.kind, Some(UsedWithKind::Modules));
    assert_eq!(view.groups.len(), 3);

    let none = service().used_with(HARPY).await.unwrap();
    assert_eq!(none.kind, None);
    assert!(none.groups.is_empty());
}

#[tokio::test]
async fn used_in_pages_with_a_next_offset() {
    let first = service().used_in(TRITANIUM, 2, 0).await.unwrap();
    assert_eq!(first.total, 3);
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.next_offset, Some(2));
    assert_eq!(first.items[0].recipe_name, "Thing 0 Blueprint");

    let last = service().used_in(TRITANIUM, 2, 2).await.unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.next_offset, None);
}

#[tokio::test]
async fn used_in_caps_the_page_size() {
    let page = service().used_in(TRITANIUM, 10_000, 0).await.unwrap();
    assert_eq!(page.limit, MAX_USED_IN_LIMIT);
}
