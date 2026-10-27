//! Effects the engine data carries no modifiers for, applied to the
//! engine's result the way pyfa applies them, so the stats agree with it.

use esf_dogma_engine::{
    AttributeValue, Calculation, EffectOperator, Fit, Source, SourceRef, State,
};

use super::EsfFittingEngine;

/// A Warp Disruption Field Generator's effect: while active, the ship's
/// signature grows by the bubble's `signatureRadiusBonus`.
const WARP_DISRUPT_SPHERE: &str = "warpDisruptSphere";

impl EsfFittingEngine {
    /// Each bubble that reached active multiplies the ship's signature by
    /// (1 + bonus / 100), in full: pyfa's `warpDisruptSphere` boosts it
    /// without a stacking penalty, after everything else. With `sources`,
    /// each bubble is listed as a source, applied or not.
    pub(super) fn apply_missing_effects(
        &self,
        fit: &Fit,
        calculation: &mut Calculation,
        sources: bool,
    ) {
        let bonus_id = self.item_attributes.signature_radius_bonus;
        let signature_id = self.attributes.signature_radius;
        let mut blooms = Vec::new();
        for (index, (item, result)) in fit.items.iter().zip(&calculation.items).enumerate() {
            let Some(effect_id) = self.effect_id(item.type_id, WARP_DISRUPT_SPHERE) else {
                continue;
            };
            let bonus = result
                .attributes
                .get(&bonus_id)
                .map_or(0.0, |attribute| attribute.value);
            blooms.push(Source {
                from: SourceRef::Item { index },
                effect_id: Some(effect_id),
                source_attribute_id: Some(bonus_id),
                operator: EffectOperator::PostPercent,
                value: bonus,
                quantity: 1,
                penalty: None,
                applied: result.state >= State::Active,
            });
        }
        if blooms.is_empty() {
            return;
        }
        let Some(signature) = calculation.ship.attributes.get_mut(&signature_id) else {
            return;
        };
        bloom(signature, blooms, sources);
    }
}

fn bloom(signature: &mut AttributeValue, blooms: Vec<Source>, sources: bool) {
    for source in &blooms {
        if source.applied {
            signature.value *= 1.0 + source.value / 100.0;
        }
    }
    if sources {
        signature.sources.extend(blooms);
    }
}
