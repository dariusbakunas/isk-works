use super::*;

/// Where the user intends to buy or sell -- a region, optionally narrowed
/// to one location/station within it. A plain value, not a persisted
/// entity: nothing owns or creates a `MarketScope` -- it's just what a caller passes to say which market it
/// means. `location_id: None` means region-wide/"all locations".
/// Deliberately excludes `solar_system_id` -- it's a display/lookup detail,
/// not part of scope identity (region + optional location is the whole
/// selection a user makes) -- and deliberately carries no `PriceSourceId`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketScope {
    pub region_id: i64,
    pub location_id: Option<i64>,
}

/// Jita 4-4 -- the well-known landing scope offered as a shortcut/default,
/// and the deserialization-compatibility default for
/// `DraftPlanningInput`/`PreviewBuildPlanCommand`'s `material_scope`/
/// `output_scope` fields: a build draft saved before those fields
/// existed simply defaults to Jita rather than failing to load.
pub const DEFAULT_MARKET_SCOPE: MarketScope = MarketScope {
    region_id: 10_000_002,
    location_id: Some(60_003_760),
};

impl MarketScope {
    /// The region whose app-wide public order book serves this scope, or
    /// `None` when the scope must stay per workspace. Region-wide scopes and
    /// NPC stations read the public regional book (a station filters it by
    /// location); structures need a workspace's own character and their
    /// books are private, and an unknown or unclassified location is never
    /// assumed public. `location` is the classification of `location_id`
    /// (ignored for a region-wide scope).
    #[must_use]
    pub fn public_region(self, location: Option<MarketLocationClassification>) -> Option<i64> {
        match (self.location_id, location) {
            (None, _) | (Some(_), Some(MarketLocationClassification::NpcStation)) => {
                Some(self.region_id)
            }
            _ => None,
        }
    }
}

#[must_use]
pub fn default_market_scope() -> MarketScope {
    DEFAULT_MARKET_SCOPE
}

/// Honest freshness state for one `(region_id, location_id)` scope --
/// deliberately not a single "last updated" timestamp. Coverage is tracked
/// per `type_id`, and a scope can have many tracked types registered at
/// very different times (a user browsing one item today, another next
/// month); `MAX(observed_at)` across all of them would report only the
/// single freshest tracked item, silently implying the whole scope is that
/// fresh even when most of it is much older or never fetched at all. A
/// caller must use `observed_type_count`/`tracked_type_count` together with
/// `most_recent_observed_at` to render an honest picture (e.g. "12 of 340
/// items observed, most recently 3m ago") -- never present
/// `most_recent_observed_at` alone as "this scope's freshness".
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeFreshness {
    /// How many distinct `type_id`s are currently registered/tracked for
    /// this scope, across every refresh state.
    pub tracked_type_count: i64,
    /// Of those, how many currently hold at least one successfully
    /// completed observation. A type whose most recent refresh *attempt*
    /// failed still counts here if an earlier attempt succeeded -- a failed
    /// retry doesn't erase previously-observed data, so it shouldn't erase
    /// this count either.
    pub observed_type_count: i64,
    /// The most recent successful observation across every tracked type in
    /// this scope. See this struct's own doc comment for why this must
    /// never be shown without `observed_type_count`/`tracked_type_count`
    /// alongside it.
    pub most_recent_observed_at: Option<DateTime<Utc>>,
}

/// The identity of the market evidence one valuation was resolved against,
/// for a single `MarketScope`. Resolved once at the start of a multi-node
/// valuation (a Build Graph request) and reused for every node on that
/// scope, so a refresh completing mid-request cannot make node B price
/// against fresher data than node A. `as_of` is the frozen wall clock used
/// for freshness/staleness classification.
///
/// A scope with no configured/observed data yields an *unpinned* evidence
/// (`as_of` only, empty batch lists) -- pricing then behaves exactly as an
/// un-pinned read, so callers can always attach one without changing
/// semantics for scopes that have no evidence.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketScopeEvidence {
    pub scope: MarketScope,
    /// Completed ESI observation batches current for this scope's sources at
    /// resolution time. Empty when the scope has no ESI coverage.
    #[serde(default)]
    pub observation_batch_ids: Vec<MarketObservationBatchId>,
    /// Latest market-import batch for this scope's locations at resolution
    /// time. `None` when the scope has no imported data.
    #[serde(default)]
    pub import_batch_id: Option<MarketImportBatchId>,
    /// Newest `observed_at` across the pinned evidence -- what this
    /// valuation "sees" of the market.
    #[serde(default)]
    pub observed_at: Option<DateTime<Utc>>,
    /// Frozen wall clock captured when the evidence was resolved; used in
    /// place of `Utc::now()` for every freshness/staleness classification
    /// on this scope, so all nodes classify identically.
    pub as_of: DateTime<Utc>,
}

impl MarketScopeEvidence {
    /// Evidence for a scope with nothing to pin: reads behave exactly as an
    /// un-pinned live read, but still share one frozen `as_of`.
    #[must_use]
    pub fn unpinned(scope: MarketScope, as_of: DateTime<Utc>) -> Self {
        Self {
            scope,
            observation_batch_ids: Vec::new(),
            import_batch_id: None,
            observed_at: None,
            as_of,
        }
    }
}

/// The request-scoped market-evidence context for one multi-node valuation.
/// Holds one [`MarketScopeEvidence`] per distinct scope actually used. Only
/// a coordinator with repository access constructs this (by resolving each
/// scope once up front); it is then passed by shared reference into
/// `preview_plan`, so a caller cannot accidentally interleave pinned and
/// un-pinned reads -- either the whole valuation carries the context or it
/// does not.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct GraphMarketEvidence {
    by_scope: Vec<MarketScopeEvidence>,
}

impl GraphMarketEvidence {
    #[must_use]
    pub fn new(by_scope: Vec<MarketScopeEvidence>) -> Self {
        Self { by_scope }
    }

    /// Add (or replace) the evidence for one scope.
    pub fn set(&mut self, evidence: MarketScopeEvidence) {
        match self
            .by_scope
            .iter_mut()
            .find(|existing| existing.scope == evidence.scope)
        {
            Some(existing) => *existing = evidence,
            None => self.by_scope.push(evidence),
        }
    }

    /// The evidence to pin reads for `scope` to, if this context has it.
    #[must_use]
    pub fn get(&self, scope: MarketScope) -> Option<&MarketScopeEvidence> {
        self.by_scope
            .iter()
            .find(|evidence| evidence.scope == scope)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_scope.is_empty()
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<MarketScopeEvidence> {
        self.by_scope
    }

    #[must_use]
    pub fn as_slice(&self) -> &[MarketScopeEvidence] {
        &self.by_scope
    }
}

#[must_use]
pub fn market_freshness(
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
    fresh_after_hours: u32,
    stale_after_hours: u32,
) -> MarketFreshnessState {
    let age = now.signed_duration_since(observed_at);
    if age.num_seconds() < 0 || age.num_hours() < i64::from(fresh_after_hours) {
        MarketFreshnessState::Fresh
    } else if age.num_hours() < i64::from(stale_after_hours) {
        MarketFreshnessState::Aging
    } else {
        MarketFreshnessState::Stale
    }
}
