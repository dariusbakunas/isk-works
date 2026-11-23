use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;

/// Past this many attempts the confident figure is not worked out.
const MAX_ATTEMPTS: u64 = 5_000_000;

/// How many attempts to plan for a number of successful copies.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttemptPlan {
    /// `ceil(copies ÷ chance)`: the long-run average.
    pub expected: u64,
    /// The fewest attempts that yield at least `copies` successes with the
    /// asked-for confidence. Never below `expected`.
    pub confident: u64,
}

/// `None` when the chance is zero or the plan would exceed five million
/// attempts. `confidence` is a probability, such as 0.9.
#[must_use]
pub fn attempts_for(copies: u64, chance: Decimal, confidence: Decimal) -> Option<AttemptPlan> {
    if copies == 0 {
        return Some(AttemptPlan {
            expected: 0,
            confident: 0,
        });
    }
    if chance <= Decimal::ZERO {
        return None;
    }
    let chance = chance.min(Decimal::ONE);
    let expected = (Decimal::from(copies) / chance).ceil().to_u64()?;
    if chance == Decimal::ONE {
        return Some(AttemptPlan {
            expected: copies,
            confident: copies,
        });
    }
    let confident = confident_attempts(copies, chance.to_f64()?, confidence.to_f64()?)?;
    Some(AttemptPlan {
        expected,
        confident: confident.max(expected),
    })
}

/// The smallest `n` with `P(successes in n attempts ≥ copies) ≥ confidence`.
///
/// Walks `n` up from `copies`, using
/// `P(X_{n+1} ≥ c) = P(X_n ≥ c) + P(X_n = c − 1) × p`. The point mass is
/// kept as a logarithm so a large target does not underflow.
fn confident_attempts(copies: u64, chance: f64, confidence: f64) -> Option<u64> {
    let confidence = confidence.clamp(0.0, 0.999_999);
    let failure = 1.0 - chance;
    let needed = copies as f64;
    // At n = copies − 1: every attempt so far succeeded.
    let mut ln_one_short = (needed - 1.0) * chance.ln();
    let mut reached = 0.0_f64;
    let mut attempts = copies - 1;
    while attempts < MAX_ATTEMPTS {
        reached += (ln_one_short + chance.ln()).exp();
        attempts += 1;
        if reached >= confidence {
            return Some(attempts);
        }
        // P(X_n = c − 1) from n − 1 to n: × n / (n − c + 1) × (1 − p).
        let n = attempts as f64;
        ln_one_short += (n / (n - needed + 1.0)).ln() + failure.ln();
    }
    None
}
