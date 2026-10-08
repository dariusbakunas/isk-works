pub(crate) mod admin;
pub(crate) mod assets;
pub(crate) mod auth;
pub(crate) mod builds;
pub(crate) mod calendar;
pub(crate) mod characters;
#[cfg(all(feature = "dev-auth", debug_assertions))]
pub(crate) mod dev_auth;
pub(crate) mod esi;
pub(crate) mod facilities;
pub(crate) mod finance;
pub(crate) mod finance_analytics;
pub(crate) mod inventory;
pub(crate) mod market;
pub(crate) mod opportunities;
pub(crate) mod order_acquisition_runs;
pub(crate) mod orders;
pub(crate) mod planetary;
pub(crate) mod sde;
pub(crate) mod workspace;

/// Hard ceiling on any SDE / reference / search `limit`. These endpoints
/// feed type-ahead pickers (the UI asks for ~20), so the real usage is far
/// below this — the cap exists so an authenticated client can't ask for an
/// unbounded scan with `?limit=4000000000`.
pub(crate) const MAX_SEARCH_LIMIT: u32 = 200;

/// Resolve a caller-supplied `limit`: fall back to `default` when absent,
/// and never exceed [`MAX_SEARCH_LIMIT`].
pub(crate) fn clamp_search_limit(requested: Option<u32>, default: u32) -> u32 {
    requested.unwrap_or(default).min(MAX_SEARCH_LIMIT)
}

#[cfg(test)]
mod search_limit_tests {
    use super::{clamp_search_limit, MAX_SEARCH_LIMIT};

    #[test]
    fn missing_limit_uses_the_endpoint_default() {
        assert_eq!(clamp_search_limit(None, 20), 20);
    }

    #[test]
    fn a_reasonable_limit_is_preserved() {
        assert_eq!(clamp_search_limit(Some(75), 20), 75);
        assert_eq!(
            clamp_search_limit(Some(MAX_SEARCH_LIMIT), 20),
            MAX_SEARCH_LIMIT
        );
    }

    #[test]
    fn an_absurd_limit_is_clamped() {
        assert_eq!(clamp_search_limit(Some(u32::MAX), 20), MAX_SEARCH_LIMIT);
        assert_eq!(clamp_search_limit(Some(10_000), 50), MAX_SEARCH_LIMIT);
    }
}
