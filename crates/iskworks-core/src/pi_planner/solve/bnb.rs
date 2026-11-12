//! The branch and bound over the colony counts: the LP relaxation (from
//! [`Model`]) gives fractional counts, and this search rounds them to the
//! best whole plan it finds within [`NODE_LIMIT`] nodes.

use std::collections::BTreeMap;

use super::{Model, Relaxed, SolveError};

/// Branch-and-bound nodes per solve; past this the best plan found so far
/// stands. Keeps every solve (and the what-ifs' dozens) bounded.
const NODE_LIMIT: usize = 250;

/// The colony count furthest from a whole number, if any isn't whole.
fn most_fractional(counts: &[f64]) -> Option<(usize, f64)> {
    counts
        .iter()
        .enumerate()
        .map(|(index, value)| {
            (
                index,
                *value,
                (value - value.floor()).min(value.ceil() - value),
            )
        })
        .filter(|(_, _, distance)| *distance > 1e-6)
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(index, value, _)| (index, value))
}

/// The best plan without factory storage, then the best with it if that
/// beats it. Allowing storage widens the search, and the node limit could
/// otherwise leave a plan worse than one that needs none.
pub(super) fn branch_and_bound(model: &Model<'_>) -> Result<Relaxed, SolveError> {
    let with_storage = model.bounds();
    let mut without_storage = with_storage.clone();
    if let Some(storage) = without_storage.last_mut() {
        storage.1 = 0.0;
    }
    let without = search(model, without_storage, None)?;
    search(model, with_storage, Some(without))
}

/// Depth-first branch and bound over the whole counts, nearest rounding
/// first, pruned by the relaxation's bound and by `incumbent`. Stops at
/// [`NODE_LIMIT`] with the best whole plan found, else the root rounded down.
fn search(
    model: &Model<'_>,
    root_bounds: Vec<(f64, f64)>,
    incumbent: Option<Relaxed>,
) -> Result<Relaxed, SolveError> {
    let root = model
        .relax(&root_bounds)
        .ok_or_else(|| SolveError::Solver("the relaxation is infeasible".to_string()))?;
    // Doing nothing is always possible and earns nothing.
    let mut best_objective = incumbent
        .as_ref()
        .map_or(0.0, |plan| plan.objective.max(0.0));
    let mut best = incumbent;
    let mut stack = vec![(root_bounds, Some(root.clone()))];
    let mut nodes = 0;
    while let Some((bounds, relaxed)) = stack.pop() {
        if nodes >= NODE_LIMIT {
            break;
        }
        nodes += 1;
        let Some(relaxed) = relaxed.or_else(|| model.relax(&bounds)) else {
            continue;
        };
        if relaxed.objective <= best_objective + 1e-6 {
            continue;
        }
        match most_fractional(&relaxed.counts) {
            None => {
                best_objective = relaxed.objective;
                best = Some(relaxed);
            }
            Some((index, value)) => {
                let mut down = bounds.clone();
                down[index].1 = value.floor();
                let mut up = bounds;
                up[index].0 = value.ceil();
                // The nearer side is explored first, so it goes on last.
                if value - value.floor() >= 0.5 {
                    stack.push((down, None));
                    stack.push((up, None));
                } else {
                    stack.push((up, None));
                    stack.push((down, None));
                }
            }
        }
    }
    if let Some(best) = best {
        return Ok(best);
    }
    let floored = root
        .counts
        .iter()
        .map(|count| (count.floor(), count.floor()))
        .collect::<Vec<_>>();
    Ok(model.relax(&floored).unwrap_or(Relaxed {
        objective: 0.0,
        counts: vec![0.0; root.counts.len()],
        extracted: root
            .extracted
            .iter()
            .map(|units| vec![0.0; units.len()])
            .collect(),
        made: BTreeMap::new(),
        sold: BTreeMap::new(),
        bought: BTreeMap::new(),
    }))
}
