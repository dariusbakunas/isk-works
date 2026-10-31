//! A product's whole production chain, per hour, down to raw resources.

use std::collections::BTreeMap;

use super::catalogue::PiCatalogue;

/// Everything one product rate needs, per hour.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainDemand {
    pub product: i64,
    pub rate_per_hour: f64,
    /// Every commodity in the chain, the product and raw resources included.
    pub per_hour: BTreeMap<i64, f64>,
    /// Factories needed per recipe output, as a fraction.
    pub factories: BTreeMap<i64, f64>,
}

impl ChainDemand {
    /// The raw resources (P0) the chain consumes, per hour.
    #[must_use]
    pub fn raw_resources(&self, catalogue: &PiCatalogue) -> BTreeMap<i64, f64> {
        self.per_hour
            .iter()
            .filter(|(type_id, _)| catalogue.tier(**type_id) == Some(0))
            .map(|(type_id, rate)| (*type_id, *rate))
            .collect()
    }

    /// Whole factories per recipe output; a factory can't run part time.
    #[must_use]
    pub fn factory_counts(&self) -> BTreeMap<i64, u32> {
        self.factories
            .iter()
            .map(|(output, count)| (*output, round_up(*count)))
            .collect()
    }
}

/// Rounds up, ignoring floating-point dust just above a whole number.
fn round_up(count: f64) -> u32 {
    let rounded = count.round();
    if (count - rounded).abs() < 1e-9 {
        rounded as u32
    } else {
        count.ceil() as u32
    }
}

/// The chain behind `rate_per_hour` of `product`; `None` when the product
/// (or anything under it) is neither a recipe output nor a raw resource.
#[must_use]
pub fn expand(catalogue: &PiCatalogue, product: i64, rate_per_hour: f64) -> Option<ChainDemand> {
    let mut demand = ChainDemand {
        product,
        rate_per_hour,
        per_hour: BTreeMap::new(),
        factories: BTreeMap::new(),
    };
    add(catalogue, product, rate_per_hour, &mut demand)?;
    Some(demand)
}

fn add(catalogue: &PiCatalogue, type_id: i64, rate: f64, demand: &mut ChainDemand) -> Option<()> {
    *demand.per_hour.entry(type_id).or_default() += rate;
    if catalogue.tier(type_id)? == 0 {
        return Some(());
    }
    let recipe = catalogue.recipe(type_id)?;
    let factories = rate / recipe.output_per_hour();
    *demand.factories.entry(type_id).or_default() += factories;
    for (input, quantity) in &recipe.inputs {
        add(
            catalogue,
            *input,
            factories * quantity / recipe.cycle_hours,
            demand,
        )?;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;

    #[test]
    fn robotics_expands_to_its_factories_and_raw_resources() {
        let catalogue = catalogue();
        let chain = expand(&catalogue, ROBOTICS, 3.0).unwrap();

        assert_eq!(chain.per_hour[&MECHANICAL_PARTS], 10.0);
        assert_eq!(chain.per_hour[&CONSUMER_ELECTRONICS], 10.0);
        assert_eq!(chain.per_hour[&REACTIVE_METALS], 80.0);
        assert_eq!(chain.per_hour[&CHIRAL_STRUCTURES], 80.0);
        assert_eq!(
            chain.factory_counts(),
            BTreeMap::from([
                (REACTIVE_METALS, 2),
                (PRECIOUS_METALS, 2),
                (TOXIC_METALS, 2),
                (CHIRAL_STRUCTURES, 2),
                (MECHANICAL_PARTS, 2),
                (CONSUMER_ELECTRONICS, 2),
                (ROBOTICS, 1),
            ])
        );
        // 40 P1 an hour takes 6 000 P0 an hour.
        assert_eq!(
            chain.raw_resources(&catalogue),
            BTreeMap::from([
                (BASE_METALS, 12_000.0),
                (NOBLE_METALS, 12_000.0),
                (HEAVY_METALS, 12_000.0),
                (NON_CS_CRYSTALS, 12_000.0),
            ])
        );
    }

    #[test]
    fn fractional_demand_rounds_factories_up() {
        let chain = expand(&catalogue(), WATER, 50.0).unwrap();
        assert_eq!(chain.factories[&WATER], 1.25);
        assert_eq!(chain.factory_counts()[&WATER], 2);
        assert_eq!(chain.per_hour[&AQUEOUS_LIQUIDS], 7_500.0);
    }

    #[test]
    fn unknown_products_have_no_chain() {
        assert!(expand(&catalogue(), 999, 1.0).is_none());
    }
}
