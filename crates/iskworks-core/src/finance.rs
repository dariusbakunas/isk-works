use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{ConnectedCharacterId, IndustryError, Money};

pub const DEFAULT_FINANCE_PAGE_SIZE: u32 = 250;
pub const MAX_FINANCE_PAGE_SIZE: u32 = 1_000;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinanceTransactionType {
    MarketBuy,
    MarketSell,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinanceDirection {
    All,
    Income,
    Expense,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinanceSortColumn {
    Time,
    Character,
    TransactionType,
    Item,
    Quantity,
    UnitPrice,
    TotalPrice,
    Direction,
    Counterparty,
    Location,
    Region,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceTransactionSort {
    pub column: FinanceSortColumn,
    pub direction: SortDirection,
}

impl Default for FinanceTransactionSort {
    fn default() -> Self {
        Self {
            column: FinanceSortColumn::Time,
            direction: SortDirection::Desc,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceTransactionFilter {
    pub connection_ids: Vec<ConnectedCharacterId>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub search: Option<String>,
    pub transaction_types: Vec<FinanceTransactionType>,
    pub direction: FinanceDirection,
    /// Analytics category label (see `FINANCE_CATEGORIES`).
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub location_id: Option<i64>,
    #[serde(default)]
    pub type_id: Option<i64>,
    /// Leave out buys currently recorded into accounting Inventory.
    #[serde(default)]
    pub exclude_inventory_buys: bool,
    pub page: u32,
    pub page_size: u32,
}

impl FinanceTransactionFilter {
    pub fn validate(mut self) -> Result<Self, FinanceError> {
        if self.page == 0 {
            return Err(FinanceError::Validation(
                "page must be greater than zero".to_string(),
            ));
        }
        if self.page_size == 0 || self.page_size > MAX_FINANCE_PAGE_SIZE {
            return Err(FinanceError::Validation(format!(
                "page size must be between 1 and {MAX_FINANCE_PAGE_SIZE}"
            )));
        }
        if self
            .date_from
            .zip(self.date_to)
            .is_some_and(|(from, to)| from > to)
        {
            return Err(FinanceError::Validation(
                "date from must not be after date to".to_string(),
            ));
        }
        self.connection_ids.sort_by_key(|id| id.0);
        self.connection_ids.dedup();
        self.transaction_types.sort_by_key(|kind| match kind {
            FinanceTransactionType::MarketBuy => 0,
            FinanceTransactionType::MarketSell => 1,
        });
        self.transaction_types.dedup();
        self.search = self
            .search
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.category = self
            .category
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(category) = &self.category {
            if !crate::FINANCE_CATEGORIES.contains(&category.as_str()) {
                return Err(FinanceError::Validation(format!(
                    "unknown category `{category}`"
                )));
            }
        }
        Ok(self)
    }
}

impl Default for FinanceTransactionFilter {
    fn default() -> Self {
        Self {
            connection_ids: Vec::new(),
            date_from: None,
            date_to: None,
            search: None,
            transaction_types: vec![
                FinanceTransactionType::MarketBuy,
                FinanceTransactionType::MarketSell,
            ],
            direction: FinanceDirection::All,
            category: None,
            location_id: None,
            type_id: None,
            exclude_inventory_buys: false,
            page: 1,
            page_size: DEFAULT_FINANCE_PAGE_SIZE,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceTransaction {
    pub observation_id: Uuid,
    pub transaction_id: i64,
    pub connection_id: ConnectedCharacterId,
    pub character_name: String,
    pub transaction_type: FinanceTransactionType,
    pub type_id: i64,
    pub type_name: String,
    pub quantity: u64,
    pub unit_price: Money,
    pub total_price: Money,
    pub transacted_at: DateTime<Utc>,
    pub counterparty_name: Option<String>,
    pub location_name: Option<String>,
    pub region_name: Option<String>,
    /// Inventory recording state for Market Buy rows; `None` for rows that can
    /// never be recorded (Market Sell).
    pub inventory_recording: Option<FinanceInventoryRecording>,
}

/// Where a Market Buy stands relative to the accounting Inventory ledger.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinanceInventoryState {
    /// Eligible and not currently recorded.
    Unrecorded,
    /// An active recording exists.
    Recorded,
    /// Recorded before, then explicitly reverted; may be recorded again.
    Reverted,
    /// Cannot safely become a purchase (not personal, unknown item type, ...).
    Unavailable,
}

/// The read model Finance shows for one Market Buy. `recording_id`, quantity
/// and basis describe the active recording, or the latest reverted one.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceInventoryRecording {
    pub state: FinanceInventoryState,
    pub recording_id: Option<Uuid>,
    pub recorded_at: Option<DateTime<Utc>>,
    pub reverted_at: Option<DateTime<Utc>>,
    pub quantity: Option<u64>,
    pub total_basis: Option<Money>,
}

/// The most recent recording of one wallet transaction, active first.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FinanceInventoryLatestRecording {
    pub recording_id: Uuid,
    pub recorded_at: DateTime<Utc>,
    pub reverted_at: Option<DateTime<Utc>>,
    pub quantity: u64,
    pub total_basis: Money,
}

/// Persisted facts the recording state is derived from. The derivation is
/// pure and lives here so the list query and the mutation responses cannot
/// disagree about eligibility.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FinanceInventoryEvidence {
    pub is_buy: bool,
    pub is_personal: bool,
    pub type_resolved: bool,
    pub latest_recording: Option<FinanceInventoryLatestRecording>,
}

impl FinanceInventoryRecording {
    #[must_use]
    pub fn derive(evidence: FinanceInventoryEvidence) -> Option<Self> {
        if !evidence.is_buy {
            return None;
        }
        let bare = |state| Self {
            state,
            recording_id: None,
            recorded_at: None,
            reverted_at: None,
            quantity: None,
            total_basis: None,
        };
        let with = |state, recording: FinanceInventoryLatestRecording| Self {
            state,
            recording_id: Some(recording.recording_id),
            recorded_at: Some(recording.recorded_at),
            reverted_at: recording.reverted_at,
            quantity: Some(recording.quantity),
            total_basis: Some(recording.total_basis),
        };
        if let Some(recording) = evidence
            .latest_recording
            .clone()
            .filter(|recording| recording.reverted_at.is_none())
        {
            return Some(with(FinanceInventoryState::Recorded, recording));
        }
        Some(match evidence.latest_recording {
            Some(recording) => with(FinanceInventoryState::Reverted, recording),
            None if evidence.is_personal && evidence.type_resolved => {
                bare(FinanceInventoryState::Unrecorded)
            }
            None => bare(FinanceInventoryState::Unavailable),
        })
    }
}

impl FinanceTransaction {
    #[must_use]
    pub const fn direction(&self) -> FinanceDirection {
        match self.transaction_type {
            FinanceTransactionType::MarketBuy => FinanceDirection::Expense,
            FinanceTransactionType::MarketSell => FinanceDirection::Income,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceSummary {
    pub wallet_balance: Money,
    pub income: Money,
    pub expenses: Money,
    pub net_isk: Money,
    pub transaction_count: u64,
    pub average_daily_isk: Money,
}

impl FinanceSummary {
    pub fn from_transactions(
        wallet_balance: Money,
        transactions: &[FinanceTransaction],
        date_from: NaiveDate,
        date_to: NaiveDate,
    ) -> Result<Self, IndustryError> {
        let mut income = Money::zero();
        let mut expenses = Money::zero();
        for transaction in transactions {
            match transaction.transaction_type {
                FinanceTransactionType::MarketBuy => {
                    expenses = expenses.checked_add(transaction.total_price)?;
                }
                FinanceTransactionType::MarketSell => {
                    income = income.checked_add(transaction.total_price)?;
                }
            }
        }
        let net_isk = income.checked_sub(expenses)?;
        let inclusive_days = date_to
            .signed_duration_since(date_from)
            .num_days()
            .checked_add(1)
            .and_then(|days| u64::try_from(days).ok())
            .filter(|days| *days > 0)
            .ok_or(IndustryError::MoneyOverflow)?;
        Ok(Self {
            wallet_balance,
            income,
            expenses,
            net_isk,
            transaction_count: transactions.len() as u64,
            average_daily_isk: net_isk.checked_div_quantity(inclusive_days)?,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceCharacter {
    pub connection_id: ConnectedCharacterId,
    pub character_name: String,
    pub wallet_balance: Option<Money>,
    pub balance_observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceTransactionPage {
    pub rows: Vec<FinanceTransaction>,
    pub summary: FinanceSummary,
    pub available_characters: Vec<FinanceCharacter>,
    pub total_count: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SavedFinanceFilterId(pub Uuid);

impl SavedFinanceFilterId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SavedFinanceFilterId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFinanceFilter {
    pub id: SavedFinanceFilterId,
    pub name: String,
    pub filter: FinanceTransactionFilter,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SavedFinanceFilter {
    pub fn validate_name(name: &str) -> Result<String, FinanceError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(FinanceError::Validation(
                "saved filter name must contain 1 to 80 characters".to_string(),
            ));
        }
        Ok(name.to_string())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinanceColumn {
    Time,
    Character,
    TransactionType,
    Item,
    Quantity,
    UnitPrice,
    TotalPrice,
    Direction,
    Counterparty,
    Location,
    Region,
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum FinanceError {
    #[error("{0}")]
    Validation(String),
    #[error("finance record not found")]
    NotFound,
    #[error("finance persistence failed: {0}")]
    Persistence(String),
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, NaiveDate, Utc};
    use serde_json::json;
    use uuid::Uuid;

    use super::{
        FinanceDirection, FinanceError, FinanceSummary, FinanceTransaction,
        FinanceTransactionFilter, FinanceTransactionSort, FinanceTransactionType,
        SavedFinanceFilter,
    };
    use crate::{ConnectedCharacterId, Money};

    #[test]
    fn summary_uses_signed_market_direction_and_inclusive_days() {
        let rows = vec![
            transaction(
                FinanceTransactionType::MarketSell,
                "120.0000",
                "2026-08-01T12:00:00Z",
            ),
            transaction(
                FinanceTransactionType::MarketBuy,
                "30.0000",
                "2026-08-03T12:00:00Z",
            ),
        ];

        let summary = FinanceSummary::from_transactions(
            Money::parse("500.0000").unwrap(),
            &rows,
            date("2026-08-01"),
            date("2026-08-03"),
        )
        .unwrap();

        assert_eq!(summary.wallet_balance, Money::parse("500.0000").unwrap());
        assert_eq!(summary.income, Money::parse("120.0000").unwrap());
        assert_eq!(summary.expenses, Money::parse("30.0000").unwrap());
        assert_eq!(summary.net_isk, Money::parse("90.0000").unwrap());
        assert_eq!(summary.average_daily_isk, Money::parse("30.0000").unwrap());
        assert_eq!(summary.transaction_count, 2);
    }

    #[test]
    fn summary_preserves_a_negative_net_value() {
        let rows = vec![transaction(
            FinanceTransactionType::MarketBuy,
            "30.0000",
            "2026-08-01T12:00:00Z",
        )];

        let summary = FinanceSummary::from_transactions(
            Money::zero(),
            &rows,
            date("2026-08-01"),
            date("2026-08-01"),
        )
        .unwrap();

        assert_eq!(summary.net_isk.0.to_string(), "-30.0000");
        assert_eq!(summary.average_daily_isk.0.to_string(), "-30.0000");
    }

    #[test]
    fn filter_validation_normalizes_search_and_duplicate_selections() {
        let connection_id = ConnectedCharacterId::new();
        let filter = FinanceTransactionFilter {
            connection_ids: vec![connection_id, connection_id],
            search: Some("  Tritanium  ".to_string()),
            transaction_types: vec![
                FinanceTransactionType::MarketSell,
                FinanceTransactionType::MarketBuy,
                FinanceTransactionType::MarketSell,
            ],
            ..FinanceTransactionFilter::default()
        }
        .validate()
        .unwrap();

        assert_eq!(filter.connection_ids, vec![connection_id]);
        assert_eq!(filter.search.as_deref(), Some("Tritanium"));
        assert_eq!(
            filter.transaction_types,
            vec![
                FinanceTransactionType::MarketBuy,
                FinanceTransactionType::MarketSell,
            ]
        );
    }

    #[test]
    fn filter_validation_accepts_known_categories_and_rejects_unknown() {
        let ok = FinanceTransactionFilter {
            category: Some("  Fuel Blocks ".to_string()),
            location_id: Some(60003760),
            type_id: Some(34),
            ..FinanceTransactionFilter::default()
        }
        .validate()
        .unwrap();
        assert_eq!(ok.category.as_deref(), Some("Fuel Blocks"));

        let blank = FinanceTransactionFilter {
            category: Some("   ".to_string()),
            ..FinanceTransactionFilter::default()
        }
        .validate()
        .unwrap();
        assert_eq!(blank.category, None);

        let unknown = FinanceTransactionFilter {
            category: Some("Nonsense".to_string()),
            ..FinanceTransactionFilter::default()
        };
        assert!(matches!(
            unknown.validate(),
            Err(FinanceError::Validation(_))
        ));
    }

    #[test]
    fn saved_filters_without_the_new_fields_still_deserialize() {
        let json = serde_json::json!({
            "connectionIds": [], "dateFrom": null, "dateTo": null, "search": null,
            "transactionTypes": ["marketBuy"], "direction": "all", "page": 1, "pageSize": 100
        });
        let filter: FinanceTransactionFilter = serde_json::from_value(json).unwrap();
        assert_eq!(filter.category, None);
        assert_eq!(filter.location_id, None);
        assert_eq!(filter.type_id, None);
    }

    #[test]
    fn filter_validation_rejects_inverted_dates_and_invalid_pages() {
        let inverted = FinanceTransactionFilter {
            date_from: Some(date("2026-08-03")),
            date_to: Some(date("2026-08-01")),
            ..FinanceTransactionFilter::default()
        };
        assert!(inverted.validate().is_err());

        let zero_page = FinanceTransactionFilter {
            page: 0,
            ..FinanceTransactionFilter::default()
        };
        assert!(zero_page.validate().is_err());
    }

    #[test]
    fn finance_contract_serializes_with_camel_case_names() {
        assert_eq!(
            serde_json::to_value(FinanceTransactionSort::default()).unwrap(),
            json!({"column": "time", "direction": "desc"})
        );
        assert_eq!(
            serde_json::to_value(FinanceDirection::Expense).unwrap(),
            json!("expense")
        );
    }

    #[test]
    fn saved_filter_names_are_trimmed_and_bounded() {
        assert_eq!(
            SavedFinanceFilter::validate_name("  Market activity  ").unwrap(),
            "Market activity"
        );
        assert!(SavedFinanceFilter::validate_name(" ").is_err());
        assert!(SavedFinanceFilter::validate_name(&"x".repeat(81)).is_err());
    }

    fn transaction(
        transaction_type: FinanceTransactionType,
        total_price: &str,
        transacted_at: &str,
    ) -> FinanceTransaction {
        FinanceTransaction {
            observation_id: Uuid::new_v4(),
            transaction_id: 1,
            connection_id: ConnectedCharacterId::new(),
            character_name: "Valka".to_string(),
            transaction_type,
            type_id: 34,
            type_name: "Tritanium".to_string(),
            quantity: 1,
            unit_price: Money::parse(total_price).unwrap(),
            total_price: Money::parse(total_price).unwrap(),
            transacted_at: DateTime::parse_from_rfc3339(transacted_at)
                .unwrap()
                .with_timezone(&Utc),
            counterparty_name: None,
            location_name: None,
            region_name: None,
            inventory_recording: None,
        }
    }

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }
}
