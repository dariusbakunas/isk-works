use super::*;

/// A blueprint's activities, in [`BlueprintActivityKind`] order: the
/// recipes the planner and the Invention page read, and what the Type Info
/// window lists. An activity naming a type the SDE has
/// no name for is left out whole. Kept types are marked referenced so they
/// survive the published-types filter.
pub(super) fn normalize_blueprint_activities(
    blueprint: &RawBlueprint,
    raw_types_by_id: &HashMap<i64, RawType>,
    referenced_type_ids: &mut HashSet<i64>,
) -> Vec<ImportBlueprintActivity> {
    let named = |type_id: i64| {
        raw_types_by_id
            .get(&type_id)
            .is_some_and(|item| item.name.en.is_some())
    };
    BlueprintActivityKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let activity = blueprint.activities.get(kind.as_str())?;
            let mut type_ids = HashSet::new();
            let materials = normalize_quantities(
                "activity material",
                blueprint.key,
                &activity.materials,
                raw_types_by_id,
                &mut type_ids,
            )
            .ok()?
            .into_iter()
            .map(|(type_id, quantity, position)| ImportMaterial {
                type_id,
                quantity,
                position,
            })
            .collect::<Vec<_>>();

            let mut seen = HashSet::new();
            let products = activity
                .products
                .iter()
                .filter(|product| product.quantity > 0 && seen.insert(product.type_id))
                .enumerate()
                .map(|(position, product)| ImportActivityProduct {
                    type_id: product.type_id,
                    quantity: product.quantity,
                    probability: product
                        .probability
                        .clone()
                        .and_then(json_number_decimal)
                        .filter(|chance| *chance > Decimal::ZERO && *chance <= Decimal::ONE),
                    position: position as i32,
                })
                .collect::<Vec<_>>();

            let mut seen = HashSet::new();
            let skills = activity
                .skills
                .iter()
                .filter(|skill| seen.insert(skill.type_id))
                .enumerate()
                .map(|(position, skill)| ImportActivitySkill {
                    skill_type_id: skill.type_id,
                    level: skill.level.clamp(1, 5),
                    position: position as i32,
                })
                .collect::<Vec<_>>();

            type_ids.extend(products.iter().map(|product| product.type_id));
            type_ids.extend(skills.iter().map(|skill| skill.skill_type_id));
            if !type_ids.iter().all(|type_id| named(*type_id)) {
                return None;
            }
            referenced_type_ids.insert(blueprint.key);
            referenced_type_ids.extend(type_ids);
            Some(ImportBlueprintActivity {
                blueprint_type_id: blueprint.key,
                kind,
                duration_seconds: activity.time.filter(|seconds| *seconds > 0),
                materials,
                products,
                skills,
            })
        })
        .collect()
}
