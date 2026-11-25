use super::*;

/// A blueprint's activities, in [`BlueprintActivityKind`] order: the
/// recipes the planner and the Invention page read, and what the Type Info
/// window lists. An activity whose material or product the SDE has no name
/// for is left out whole (a skill it has no name for is only dropped), and
/// so is a manufacturing, reaction or invention activity
/// that yields nothing. An invention product needs a chance in `(0, 1]`.
/// Kept types are marked referenced so they survive the published-types
/// filter.
pub(super) fn normalize_blueprint_activities(
    blueprint: &RawBlueprint,
    raw_types_by_id: &HashMap<i64, RawType>,
    referenced_type_ids: &mut HashSet<i64>,
) -> Result<Vec<ImportBlueprintActivity>, SdeError> {
    let named = |type_id: i64| {
        raw_types_by_id
            .get(&type_id)
            .is_some_and(|item| item.name.en.is_some())
    };
    let mut activities = Vec::new();
    for kind in BlueprintActivityKind::ALL {
        let Some(activity) = blueprint.activities.get(kind.as_str()) else {
            continue;
        };
        let recipe = matches!(
            kind,
            BlueprintActivityKind::Manufacturing | BlueprintActivityKind::Reaction
        );
        let invention = kind == BlueprintActivityKind::Invention;
        let normalized = (|| {
            let mut type_ids = HashSet::new();
            let materials = match normalize_quantities(
                "material",
                blueprint.key,
                &activity.materials,
                raw_types_by_id,
                &mut type_ids,
            ) {
                Ok(lines) => lines,
                // A recipe with a bad quantity fails the import; elsewhere
                // the activity is left out.
                Err(error @ SdeError::InvalidQuantity { .. }) if recipe => return Err(error),
                Err(_) => return Ok(None),
            }
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
                .filter(|product| product.quantity > 0)
                .map(|product| {
                    let probability = product
                        .probability
                        .clone()
                        .and_then(json_number_decimal)
                        .filter(|chance| *chance > Decimal::ZERO && *chance <= Decimal::ONE);
                    (product, probability)
                })
                .filter(|(_, probability)| !invention || probability.is_some())
                .filter(|(product, _)| seen.insert(product.type_id))
                .enumerate()
                .map(|(position, (product, probability))| ImportActivityProduct {
                    type_id: product.type_id,
                    quantity: product.quantity,
                    probability,
                    position: position as i32,
                })
                .collect::<Vec<_>>();
            if (recipe || invention) && products.is_empty() {
                return Ok(None);
            }

            let mut seen = HashSet::new();
            let skills = activity
                .skills
                .iter()
                .filter(|skill| named(skill.type_id) && seen.insert(skill.type_id))
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
                return Ok(None);
            }
            referenced_type_ids.insert(blueprint.key);
            referenced_type_ids.extend(type_ids);
            Ok(Some(ImportBlueprintActivity {
                blueprint_type_id: blueprint.key,
                kind,
                duration_seconds: activity.time.filter(|seconds| *seconds > 0),
                materials,
                products,
                skills,
            }))
        })()?;
        activities.extend(normalized);
    }
    Ok(activities)
}
