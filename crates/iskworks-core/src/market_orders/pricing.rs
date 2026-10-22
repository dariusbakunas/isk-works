//! Valid order prices and sales tax.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Which way a price moves to beat a competitor: sell orders undercut, buy
/// orders overbid.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PriceDirection {
    Down,
    Up,
}

/// The smallest price step the market accepts around `price`: orders carry
/// at most four significant digits, and never less than 0.01 ISK.
fn tick(price: Decimal) -> Decimal {
    power_of_ten(price_exponent(price) - 3).max(Decimal::new(1, 2))
}

fn power_of_ten(exponent: i32) -> Decimal {
    if exponent >= 0 {
        (0..exponent).fold(Decimal::ONE, |value, _| value * Decimal::TEN)
    } else {
        Decimal::new(1, exponent.unsigned_abs())
    }
}

/// The step that applies just below `price`: a power of ten is the first
/// value of its decade, so the prices under it use the finer step.
fn tick_below(price: Decimal) -> Decimal {
    let step = tick(price);
    let finer = (step / Decimal::TEN).max(Decimal::new(1, 2));
    if price == power_of_ten(price_exponent(price)) {
        finer
    } else {
        step
    }
}

fn price_exponent(price: Decimal) -> i32 {
    let mut exponent = 0_i32;
    let mut value = price;
    while value >= Decimal::TEN {
        value /= Decimal::TEN;
        exponent += 1;
    }
    while value < Decimal::ONE {
        value *= Decimal::TEN;
        exponent -= 1;
    }
    exponent
}

/// The nearest valid price strictly past `price` in `direction`: the price
/// that beats an order at `price` by the smallest allowed step. Returns
/// `None` when no positive price lies below.
#[must_use]
pub fn next_price_step(price: Decimal, direction: PriceDirection) -> Option<Decimal> {
    if price <= Decimal::ZERO {
        return None;
    }
    let next = match direction {
        PriceDirection::Down => {
            let step = tick_below(price);
            let floored = (price / step).floor() * step;
            if floored < price {
                floored
            } else {
                floored - step
            }
        }
        PriceDirection::Up => {
            let step = tick(price);
            let ceiled = (price / step).ceil() * step;
            if ceiled > price {
                ceiled
            } else {
                ceiled + step
            }
        }
    };
    (next > Decimal::ZERO).then(|| next.normalize())
}

/// Sales tax before skills, as a fraction of the sale price. The same in
/// NPC stations and player structures.
// Check against current game rules when tax changes are announced.
pub const BASE_SALES_TAX: Decimal = Decimal::from_parts(75, 0, 0, false, 3);
/// Each Accounting level cuts the base rate by this fraction.
pub const ACCOUNTING_REDUCTION_PER_LEVEL: Decimal = Decimal::from_parts(11, 0, 0, false, 2);
/// The Accounting skill's type ID.
pub const ACCOUNTING_SKILL_ID: i64 = 16_622;

/// The sales tax a character pays with `accounting_level` (0-5) trained.
#[must_use]
pub fn sales_tax_rate(accounting_level: u8) -> Decimal {
    let level = Decimal::from(accounting_level.min(5));
    BASE_SALES_TAX * (Decimal::ONE - ACCOUNTING_REDUCTION_PER_LEVEL * level)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(value: &str) -> Decimal {
        value.parse().unwrap()
    }

    fn down(price: &str) -> Decimal {
        next_price_step(d(price), PriceDirection::Down).unwrap()
    }

    fn up(price: &str) -> Decimal {
        next_price_step(d(price), PriceDirection::Up).unwrap()
    }

    #[test]
    fn undercuts_by_one_step_in_the_fourth_significant_digit() {
        assert_eq!(down("2140000000"), d("2139000000"));
        assert_eq!(down("311900000"), d("311800000"));
        assert_eq!(down("2480000"), d("2479000"));
    }

    #[test]
    fn overbids_by_one_step_in_the_fourth_significant_digit() {
        assert_eq!(up("9215"), d("9216"));
        assert_eq!(up("4.21"), d("4.22"));
        assert_eq!(up("2140000000"), d("2141000000"));
    }

    #[test]
    fn crossing_a_power_of_ten_switches_the_step() {
        assert_eq!(down("1000"), d("999.9"));
        assert_eq!(up("999.9"), d("1000"));
        assert_eq!(down("1000000000"), d("999900000"));
        assert_eq!(up("9999"), d("10000"));
    }

    #[test]
    fn an_off_grid_price_rounds_to_the_nearest_valid_price_past_it() {
        assert_eq!(down("312412345"), d("312400000"));
        assert_eq!(up("312412345"), d("312500000"));
    }

    #[test]
    fn small_prices_never_step_below_a_hundredth() {
        assert_eq!(down("10"), d("9.99"));
        assert_eq!(down("0.5"), d("0.49"));
        assert_eq!(up("0.01"), d("0.02"));
        assert_eq!(next_price_step(d("0.01"), PriceDirection::Down), None);
    }

    #[test]
    fn huge_prices_keep_four_significant_digits() {
        assert_eq!(down("75000000000"), d("74990000000"));
    }

    #[test]
    fn accounting_cuts_the_base_sales_tax() {
        assert_eq!(sales_tax_rate(0), d("0.075"));
        assert_eq!(sales_tax_rate(5), d("0.033750"));
        assert_eq!(sales_tax_rate(9), sales_tax_rate(5));
    }
}
