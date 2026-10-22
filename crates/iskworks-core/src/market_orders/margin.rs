//! What a sell order earns over the stock's cost basis.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Per-unit and remaining earnings of a sell order at one price, after
/// sales tax. Broker fees are left out: the listing fee is already paid,
/// and a structure owner's rate isn't visible through ESI.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SellMargin {
    /// The price less sales tax.
    pub net_unit: Decimal,
    pub margin_unit: Decimal,
    /// `margin_unit` as a percentage of the unit cost; `None` for stock
    /// recorded at zero cost.
    pub margin_percent: Option<Decimal>,
    /// `margin_unit` for every unit still on the order.
    pub remaining_profit: Decimal,
}

#[must_use]
pub fn sell_margin(
    price: Decimal,
    unit_cost: Decimal,
    sales_tax_rate: Decimal,
    volume_remain: i64,
) -> SellMargin {
    let net_unit = (price * (Decimal::ONE - sales_tax_rate)).round_dp(2);
    let margin_unit = net_unit - unit_cost;
    SellMargin {
        net_unit,
        margin_unit,
        margin_percent: (unit_cost > Decimal::ZERO)
            .then(|| (margin_unit / unit_cost * Decimal::ONE_HUNDRED).round_dp(2)),
        remaining_profit: margin_unit * Decimal::from(volume_remain),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(value: &str) -> Decimal {
        value.parse().unwrap()
    }

    #[test]
    fn margin_is_the_taxed_price_less_the_unit_cost() {
        let margin = sell_margin(d("2149000000"), d("1620000000"), d("0.03375"), 1);
        assert_eq!(margin.net_unit, d("2076471250"));
        assert_eq!(margin.margin_unit, d("456471250"));
        assert_eq!(margin.margin_percent, Some(d("28.18")));
        assert_eq!(margin.remaining_profit, d("456471250"));
    }

    #[test]
    fn a_thin_margin_turns_into_a_loss_after_tax() {
        // 312.4m against a 305m basis looks like +2.4% before tax.
        let margin = sell_margin(d("312400000"), d("305000000"), d("0.03375"), 5);
        assert!(margin.margin_unit < Decimal::ZERO);
        assert_eq!(margin.remaining_profit, margin.margin_unit * d("5"));
    }

    #[test]
    fn zero_cost_stock_has_no_margin_percentage() {
        let margin = sell_margin(d("100"), Decimal::ZERO, d("0.075"), 3);
        assert_eq!(margin.margin_unit, d("92.50"));
        assert_eq!(margin.margin_percent, None);
    }
}
