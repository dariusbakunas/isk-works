use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PriceSourceKind {
    Manual,
    EveClientMarketExport,
    EsiMarketOrders,
}

impl PriceSourceKind {
    #[must_use]
    pub const fn uses_order_book(self) -> bool {
        matches!(self, Self::EveClientMarketExport | Self::EsiMarketOrders)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceSourceItem {
    pub type_id: i64,
    pub type_name: String,
    pub price: Money,
    pub note: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceSource {
    pub id: PriceSourceId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub description: String,
    pub kind: PriceSourceKind,
    pub revision: u64,
    pub item_count: u64,
    pub recent_build_count: u64,
    pub items: Vec<PriceSourceItem>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceSnapshotLine {
    pub type_id: i64,
    pub type_name: String,
    pub item_role: PlannerItemRole,
    pub selection_kind: PricingSelectionKind,
    pub manual_unit_price: Option<Money>,
    pub price: Option<Money>,
    pub pricing_policy: Option<crate::MarketPricingPolicy>,
    pub missing: bool,
    pub source_note: String,
    pub sort_order: u32,
    /// The market scope this line was priced against -- `None` for a line
    /// with no market pricing at all (manual-only, or missing). This is
    /// what makes a snapshot reproducible under per-role scopes, since
    /// `pricing_policy` alone doesn't imply *which* market it was
    /// evaluated against.
    pub market_region_id: Option<i64>,
    pub market_location_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingSelectionKind {
    Default,
    MarketPolicy,
    Manual,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceSnapshot {
    pub id: PriceSnapshotId,
    pub price_source_id: Option<PriceSourceId>,
    pub source_name: String,
    pub source_revision: u64,
    pub created_at: DateTime<Utc>,
    pub items: Vec<PriceSnapshotLine>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePriceSourceCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePriceSourceCommand {
    pub expected_revision: u64,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceInput {
    pub type_id: i64,
    pub type_name: String,
    pub price: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildItemPricingPolicy {
    pub type_id: i64,
    pub pricing_policy: crate::MarketPricingPolicy,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerItemRole {
    Material,
    Output,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemPricingSelection {
    Default,
    MarketPolicy { policy: crate::MarketPricingPolicy },
    Manual { unit_price: String },
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPricingSelectionInput {
    pub type_id: i64,
    pub role: PlannerItemRole,
    pub selection: ItemPricingSelection,
}
