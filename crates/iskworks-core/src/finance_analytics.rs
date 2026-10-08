//! Finance analytics: the query, the aggregated payload, and the pure logic
//! that turns raw SQL aggregates into it (bucketing, previous-period windows,
//! deltas, top-N folding, wallet-balance history, rule-based insights).
//!
//! "Income" is market sells and "expenses" is market buys; nothing here knows
//! about the wallet journal yet.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::finance_categories::CATEGORY_OTHER;
use crate::{
    ConnectedCharacterId, FinanceCharacter, FinanceDirection, FinanceError, FinanceTransactionType,
    Money, WorkspaceId,
};

pub const TOP_CATEGORY_COUNT: usize = 8;
pub const TOP_LOCATION_COUNT: usize = 8;
pub const TOP_ITEM_COUNT: usize = 10;
pub const TREND_POINTS: usize = 8;
pub const HEATMAP_DAYS: i64 = 91;
pub const MAX_ANALYTICS_RANGE_DAYS: i64 = 731;

pub const LOCATION_CONCENTRATION_PERCENT: f64 = 50.0;
pub const CHARACTER_CONCENTRATION_PERCENT: f64 = 40.0;
pub const CATEGORY_CHANGE_MIN_PERCENT: f64 = 25.0;
pub const CATEGORY_CHANGE_MIN_SHARE_PERCENT: f64 = 3.0;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Granularity {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FinanceAnalyticsQuery {
    /// Empty means every registered character.
    pub connection_ids: Vec<ConnectedCharacterId>,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub include_income: bool,
    pub include_expenses: bool,
    pub granularity: Granularity,
    pub compare_previous: bool,
    /// Leave out purchases currently recorded into accounting Inventory: they
    /// are build inputs, not spend.
    pub exclude_inventory_buys: bool,
    pub category: Option<String>,
}

impl FinanceAnalyticsQuery {
    /// Which sides of the ledger a Finance direction + transaction-type
    /// selection covers. Sells are income, buys are expenses.
    #[must_use]
    pub fn sides(
        direction: FinanceDirection,
        transaction_types: &[FinanceTransactionType],
    ) -> (bool, bool) {
        let sells = transaction_types.contains(&FinanceTransactionType::MarketSell);
        let buys = transaction_types.contains(&FinanceTransactionType::MarketBuy);
        (
            sells && direction != FinanceDirection::Expense,
            buys && direction != FinanceDirection::Income,
        )
    }

    pub fn validate(mut self) -> Result<Self, FinanceError> {
        if self.date_from > self.date_to {
            return Err(FinanceError::Validation(
                "date from must not be after date to".to_string(),
            ));
        }
        let days = (self.date_to - self.date_from).num_days() + 1;
        if days > MAX_ANALYTICS_RANGE_DAYS {
            return Err(FinanceError::Validation(format!(
                "date range must not exceed {MAX_ANALYTICS_RANGE_DAYS} days"
            )));
        }
        self.connection_ids.sort_by_key(|id| id.0);
        self.connection_ids.dedup();
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

    /// The equally long window that ends the day before `date_from`, when
    /// comparison is on.
    #[must_use]
    pub fn previous_window(&self) -> Option<(NaiveDate, NaiveDate)> {
        self.compare_previous
            .then(|| previous_window(self.date_from, self.date_to))
    }
}

#[must_use]
pub fn previous_window(date_from: NaiveDate, date_to: NaiveDate) -> (NaiveDate, NaiveDate) {
    let days = (date_to - date_from).num_days() + 1;
    let previous_to = date_from - Duration::days(1);
    (previous_to - Duration::days(days - 1), previous_to)
}

/// First day of the bucket `date` falls in. Weeks start on Monday.
#[must_use]
pub fn bucket_start(date: NaiveDate, granularity: Granularity) -> NaiveDate {
    match granularity {
        Granularity::Day => date,
        Granularity::Week => {
            date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
        }
        Granularity::Month => date.with_day(1).expect("day 1 exists in every month"),
    }
}

#[must_use]
pub fn next_bucket(start: NaiveDate, granularity: Granularity) -> NaiveDate {
    match granularity {
        Granularity::Day => start + Duration::days(1),
        Granularity::Week => start + Duration::days(7),
        Granularity::Month => {
            let (year, month) = if start.month() == 12 {
                (start.year() + 1, 1)
            } else {
                (start.year(), start.month() + 1)
            };
            NaiveDate::from_ymd_opt(year, month, 1).expect("first of month is valid")
        }
    }
}

// ---------------------------------------------------------------- payload

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceAnalytics {
    pub range: AnalyticsRange,
    /// Earliest wallet transaction we have stored for the selected
    /// characters, so the UI can say when history is shorter than the range.
    pub earliest_observed_at: Option<DateTime<Utc>>,
    /// Every registered character, so the page can offer them in its filter
    /// before (and regardless of) what the selected range contains.
    pub available_characters: Vec<FinanceCharacter>,
    pub excluded_intra_account: ExcludedTrades,
    pub excluded_inventory_buys: ExcludedTrades,
    pub kpis: AnalyticsKpis,
    pub cash_flow: Vec<CashFlowBucket>,
    pub spending_by_category: Vec<CategoryTotal>,
    pub income_by_category: Vec<CategoryTotal>,
    pub by_character: Vec<CharacterTotal>,
    pub by_location: Vec<LocationTotal>,
    pub top_expenses: Vec<TopItem>,
    pub top_earners: Vec<TopItem>,
    pub heatmap: Vec<DayNet>,
    pub insights: Vec<Insight>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsRange {
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub previous_date_from: Option<NaiveDate>,
    pub previous_date_to: Option<NaiveDate>,
    pub granularity: Granularity,
}

/// Trades left out of every aggregate and reported here instead: trades
/// between two of our own characters (not real income or spend), or buys
/// already recorded into Inventory when the caller asks to exclude those.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedTrades {
    pub transaction_count: u64,
    pub total_isk: Money,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Delta {
    pub previous: Money,
    /// `None` when there is no baseline to divide by (previous is zero).
    pub percent: Option<f64>,
    /// Previous was zero and current is not: the UI shows "new".
    pub is_new: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KpiValue {
    pub value: Money,
    pub delta: Option<Delta>,
    /// One point per cash-flow bucket, for the card sparkline.
    pub sparkline: Vec<Money>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginKpi {
    /// Net / income as a percentage; `None` when there was no income.
    pub percent: Option<f64>,
    pub previous_percent: Option<f64>,
    pub sparkline: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletKpi {
    pub value: Option<Money>,
    pub delta: Option<Delta>,
    /// Only present with at least two comparable snapshots.
    pub sparkline: Vec<Money>,
}

/// Taxes and fees paid on market trading, from the wallet journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeesKpi {
    pub value: Money,
    /// Withheld when the journal does not reach back over the whole previous
    /// window, since its total would be a partial figure.
    pub delta: Option<Delta>,
    /// One point per cash-flow bucket.
    pub sparkline: Vec<Money>,
    pub brokers_fee: Money,
    pub transaction_tax: Money,
    /// Structure owners' market fees.
    pub market_provider_tax: Money,
    /// First day the journal has for these characters, so the UI can say when
    /// it only covers part of the range.
    pub available_from: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsKpis {
    pub income: KpiValue,
    pub expenses: KpiValue,
    pub net: KpiValue,
    pub margin: MarginKpi,
    pub wallet_balance: WalletKpi,
    /// `None` when there is no wallet journal data for the selection, or when a
    /// category is selected (fees cannot be attributed to a category).
    pub fees: Option<FeesKpi>,
    pub transaction_count: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CashFlowBucket {
    pub start: NaiveDate,
    pub income: Money,
    pub expenses: Money,
    pub net: Money,
    pub cumulative_net: Money,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category: String,
    pub total: Money,
    pub previous: Option<Money>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterTotal {
    pub connection_id: ConnectedCharacterId,
    pub character_name: String,
    pub income: Money,
    pub expenses: Money,
    pub net: Money,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationTotal {
    pub location_id: i64,
    pub location_name: String,
    pub region_name: Option<String>,
    pub income: Money,
    pub expenses: Money,
    pub net: Money,
    pub transaction_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopItem {
    pub type_id: i64,
    pub type_name: String,
    pub category: String,
    pub quantity: u64,
    /// Total / quantity, so a weighted ISK-per-unit.
    pub average_unit_price: Money,
    pub total: Money,
    /// Share of the side's total (income for earners, spend for expenses).
    pub share_percent: Option<f64>,
    pub trend: Vec<Money>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayNet {
    pub date: NaiveDate,
    pub net: Money,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsightKind {
    CategoryChange,
    LocationConcentration,
    CharacterConcentration,
    FeeBurden,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsightSide {
    Spending,
    Income,
}

/// Structured on purpose: the numbers are computed here, the wording is the
/// client's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Insight {
    pub kind: InsightKind,
    pub side: Option<InsightSide>,
    pub subject: String,
    pub amount: Money,
    pub previous: Option<Money>,
    pub total: Option<Money>,
    pub share_percent: Option<f64>,
    pub change_percent: Option<f64>,
}

#[async_trait]
pub trait FinanceAnalyticsRepository: Send + Sync {
    async fn analytics(
        &self,
        workspace_id: WorkspaceId,
        query: FinanceAnalyticsQuery,
    ) -> Result<FinanceAnalytics, FinanceError>;
}

// ------------------------------------------------------------ pure logic

fn money_err(error: impl ToString) -> FinanceError {
    FinanceError::Persistence(error.to_string())
}

#[must_use]
pub fn percent_of(part: Money, whole: Money) -> Option<f64> {
    use rust_decimal::prelude::ToPrimitive;
    if whole.0.is_zero() {
        return None;
    }
    (part.0 * rust_decimal::Decimal::from(100) / whole.0).to_f64()
}

/// `previous == None` means comparison is off (no delta). A zero baseline
/// yields `is_new` when there is something now, and no percentage.
#[must_use]
pub fn delta(current: Money, previous: Option<Money>) -> Option<Delta> {
    use rust_decimal::prelude::ToPrimitive;
    let previous = previous?;
    if previous.0.is_zero() {
        return Some(Delta {
            previous,
            percent: None,
            is_new: !current.0.is_zero(),
        });
    }
    let percent =
        ((current.0 - previous.0) * rust_decimal::Decimal::from(100) / previous.0.abs()).to_f64();
    Some(Delta {
        previous,
        percent,
        is_new: false,
    })
}

/// One bucket per period across the range, gaps zero-filled, with a running
/// cumulative net. `sums` maps a bucket start to `(income, expenses)`.
pub fn build_cash_flow(
    date_from: NaiveDate,
    date_to: NaiveDate,
    granularity: Granularity,
    sums: &BTreeMap<NaiveDate, (Money, Money)>,
) -> Result<Vec<CashFlowBucket>, FinanceError> {
    let mut buckets = Vec::new();
    let mut cumulative = Money::zero();
    let mut start = bucket_start(date_from, granularity);
    while start <= date_to {
        let (income, expenses) = sums
            .get(&start)
            .copied()
            .unwrap_or((Money::zero(), Money::zero()));
        let net = income.checked_sub(expenses).map_err(money_err)?;
        cumulative = cumulative.checked_add(net).map_err(money_err)?;
        buckets.push(CashFlowBucket {
            start,
            income,
            expenses,
            net,
            cumulative_net: cumulative,
        });
        start = next_bucket(start, granularity);
    }
    Ok(buckets)
}

/// Keep the `limit` largest named categories and fold everything else,
/// including any existing "Other", into a single trailing "Other".
pub fn fold_top_categories(
    mut rows: Vec<CategoryTotal>,
    limit: usize,
) -> Result<Vec<CategoryTotal>, FinanceError> {
    rows.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| a.category.cmp(&b.category))
    });
    let mut kept = Vec::new();
    let mut other_total = Money::zero();
    let mut other_previous: Option<Money> = None;
    let mut has_other = false;
    for row in rows {
        if row.category != CATEGORY_OTHER && kept.len() < limit {
            kept.push(row);
            continue;
        }
        has_other = true;
        other_total = other_total.checked_add(row.total).map_err(money_err)?;
        if let Some(previous) = row.previous {
            other_previous = Some(
                other_previous
                    .unwrap_or_else(Money::zero)
                    .checked_add(previous)
                    .map_err(money_err)?,
            );
        }
    }
    if has_other {
        kept.push(CategoryTotal {
            category: CATEGORY_OTHER.to_string(),
            total: other_total,
            previous: other_previous,
        });
    }
    Ok(kept)
}

/// Wallet balance over time from sparse sync snapshots. A character with no
/// snapshot yet would understate the total, so the series only starts on the
/// first day every snapshotted character has a balance. Days without a
/// snapshot are omitted; per-character balances carry forward.
pub fn wallet_balance_series(
    snapshots: &[(ConnectedCharacterId, DateTime<Utc>, Money)],
) -> Result<Vec<(NaiveDate, Money)>, FinanceError> {
    let mut sorted: Vec<_> = snapshots.iter().collect();
    sorted.sort_by_key(|(_, at, _)| *at);
    let expected: BTreeSet<_> = snapshots.iter().map(|(id, _, _)| id.0).collect();
    let mut latest: HashMap<uuid::Uuid, Money> = HashMap::new();
    let mut series: Vec<(NaiveDate, Money)> = Vec::new();
    for (id, at, balance) in sorted {
        latest.insert(id.0, *balance);
        if latest.len() < expected.len() {
            continue;
        }
        let total = latest
            .values()
            .try_fold(Money::zero(), |sum, value| sum.checked_add(*value))
            .map_err(money_err)?;
        let day = at.date_naive();
        match series.last_mut() {
            Some(last) if last.0 == day => last.1 = total,
            _ => series.push((day, total)),
        }
    }
    Ok(series)
}

#[must_use]
pub fn balance_at_or_before(series: &[(NaiveDate, Money)], date: NaiveDate) -> Option<Money> {
    series
        .iter()
        .rev()
        .find(|(day, _)| *day <= date)
        .map(|(_, balance)| *balance)
}

/// Net per day for the trailing window ending `end`, zero-filled.
#[must_use]
pub fn fill_heatmap(end: NaiveDate, days: i64, nets: &BTreeMap<NaiveDate, Money>) -> Vec<DayNet> {
    (0..days)
        .map(|offset| {
            let date = end - Duration::days(days - 1 - offset);
            DayNet {
                date,
                net: nets.get(&date).copied().unwrap_or_else(Money::zero),
            }
        })
        .collect()
}

pub struct InsightInputs<'a> {
    pub spending: &'a [CategoryTotal],
    pub income: &'a [CategoryTotal],
    pub by_location: &'a [LocationTotal],
    pub by_character: &'a [CharacterTotal],
    pub fees: Option<&'a FeesKpi>,
    pub income_total: Money,
}

/// Simple threshold rules over the aggregates. A rule that does not fire
/// contributes nothing, so a quiet period yields no insights.
pub fn derive_insights(inputs: &InsightInputs<'_>) -> Result<Vec<Insight>, FinanceError> {
    let mut insights = Vec::new();
    if let Some(insight) = category_change(inputs)? {
        insights.push(insight);
    }
    if let Some(insight) = location_concentration(inputs.by_location)? {
        insights.push(insight);
    }
    if let Some(insight) = character_concentration(inputs.by_character)? {
        insights.push(insight);
    }
    if let Some(insight) = fee_burden(inputs)? {
        insights.push(insight);
    }
    Ok(insights)
}

/// The ledger reasons that are fees on market trading.
pub const FEE_REF_TYPES: &[&str] = &["brokers_fee", "transaction_tax", "market_provider_tax"];

/// One aggregated wallet-journal row: fees of one type, either inside the
/// current window (with its cash-flow bucket) or the previous one.
#[derive(Debug, Clone, PartialEq)]
pub struct FeeRow {
    pub ref_type: String,
    pub in_current: bool,
    pub bucket: Option<NaiveDate>,
    /// Positive = paid.
    pub total: Money,
}

/// Minimum share of income for the fee insight to be worth saying.
pub const FEE_BURDEN_MIN_PERCENT: f64 = 1.0;

pub fn build_fees(
    rows: &[FeeRow],
    bucket_starts: &[NaiveDate],
    previous_window: Option<(NaiveDate, NaiveDate)>,
    earliest_journal: Option<NaiveDate>,
) -> Result<Option<FeesKpi>, FinanceError> {
    // No journal at all: we know nothing, which is not the same as zero fees.
    let Some(available_from) = earliest_journal else {
        return Ok(None);
    };
    let (mut brokers_fee, mut transaction_tax, mut market_provider_tax) =
        (Money::zero(), Money::zero(), Money::zero());
    let mut previous = Money::zero();
    let mut by_bucket: BTreeMap<NaiveDate, Money> = BTreeMap::new();
    for row in rows {
        if !row.in_current {
            previous = previous.checked_add(row.total).map_err(money_err)?;
            continue;
        }
        let kind = match row.ref_type.as_str() {
            "brokers_fee" => &mut brokers_fee,
            "transaction_tax" => &mut transaction_tax,
            "market_provider_tax" => &mut market_provider_tax,
            _ => continue,
        };
        *kind = kind.checked_add(row.total).map_err(money_err)?;
        if let Some(bucket) = row.bucket {
            let entry = by_bucket.entry(bucket).or_insert_with(Money::zero);
            *entry = entry.checked_add(row.total).map_err(money_err)?;
        }
    }
    let value = brokers_fee
        .checked_add(transaction_tax)
        .and_then(|sum| sum.checked_add(market_provider_tax))
        .map_err(money_err)?;
    let previous_covered = previous_window.is_some_and(|(from, _)| available_from <= from);
    Ok(Some(FeesKpi {
        value,
        delta: if previous_covered {
            delta(value, Some(previous))
        } else {
            None
        },
        sparkline: bucket_starts
            .iter()
            .map(|start| by_bucket.get(start).copied().unwrap_or_else(Money::zero))
            .collect(),
        brokers_fee,
        transaction_tax,
        market_provider_tax,
        available_from: Some(available_from),
    }))
}

fn fee_burden(inputs: &InsightInputs<'_>) -> Result<Option<Insight>, FinanceError> {
    let Some(fees) = inputs.fees else {
        return Ok(None);
    };
    if fees.value.0.is_zero() {
        return Ok(None);
    }
    let share = percent_of(fees.value, inputs.income_total);
    Ok(share
        .filter(|share| *share >= FEE_BURDEN_MIN_PERCENT)
        .map(|share| Insight {
            kind: InsightKind::FeeBurden,
            side: Some(InsightSide::Spending),
            subject: "Taxes & fees".to_string(),
            amount: fees.value,
            previous: fees.delta.as_ref().map(|delta| delta.previous),
            total: Some(inputs.income_total),
            share_percent: Some(share),
            change_percent: fees.delta.as_ref().and_then(|delta| delta.percent),
        }))
}

fn sum_money<'a>(mut values: impl Iterator<Item = &'a Money>) -> Result<Money, FinanceError> {
    values
        .try_fold(Money::zero(), |sum, value| sum.checked_add(*value))
        .map_err(money_err)
}

fn category_change(inputs: &InsightInputs<'_>) -> Result<Option<Insight>, FinanceError> {
    let mut best: Option<(rust_decimal::Decimal, Insight)> = None;
    for (rows, side) in [
        (inputs.spending, InsightSide::Spending),
        (inputs.income, InsightSide::Income),
    ] {
        let total = sum_money(rows.iter().map(|row| &row.total))?;
        let previous_total = sum_money(rows.iter().filter_map(|row| row.previous.as_ref()))?;
        let scale = if total.0 > previous_total.0 {
            total
        } else {
            previous_total
        };
        for row in rows.iter().filter(|row| row.category != CATEGORY_OTHER) {
            let Some(previous) = row.previous else {
                continue;
            };
            let Some(change) = delta(row.total, Some(previous)).and_then(|d| d.percent) else {
                continue;
            };
            let larger = if row.total.0 > previous.0 {
                row.total
            } else {
                previous
            };
            let share = percent_of(larger, scale).unwrap_or(0.0);
            if change.abs() < CATEGORY_CHANGE_MIN_PERCENT
                || share < CATEGORY_CHANGE_MIN_SHARE_PERCENT
            {
                continue;
            }
            let magnitude = (row.total.0 - previous.0).abs();
            if best
                .as_ref()
                .map_or(true, |(current, _)| magnitude > *current)
            {
                best = Some((
                    magnitude,
                    Insight {
                        kind: InsightKind::CategoryChange,
                        side: Some(side),
                        subject: row.category.clone(),
                        amount: row.total,
                        previous: Some(previous),
                        total: Some(total),
                        share_percent: percent_of(row.total, total),
                        change_percent: Some(change),
                    },
                ));
            }
        }
    }
    Ok(best.map(|(_, insight)| insight))
}

fn location_concentration(rows: &[LocationTotal]) -> Result<Option<Insight>, FinanceError> {
    let with_spend: Vec<_> = rows
        .iter()
        .filter(|row| !row.expenses.0.is_zero())
        .collect();
    if with_spend.len() < 2 {
        return Ok(None);
    }
    let total = sum_money(with_spend.iter().map(|row| &row.expenses))?;
    let top = with_spend
        .iter()
        .max_by_key(|row| row.expenses)
        .expect("non-empty");
    let share = percent_of(top.expenses, total);
    Ok(share
        .filter(|share| *share >= LOCATION_CONCENTRATION_PERCENT)
        .map(|share| Insight {
            kind: InsightKind::LocationConcentration,
            side: Some(InsightSide::Spending),
            subject: top.location_name.clone(),
            amount: top.expenses,
            previous: None,
            total: Some(total),
            share_percent: Some(share),
            change_percent: None,
        }))
}

fn character_concentration(rows: &[CharacterTotal]) -> Result<Option<Insight>, FinanceError> {
    let with_income: Vec<_> = rows.iter().filter(|row| !row.income.0.is_zero()).collect();
    if with_income.len() < 2 {
        return Ok(None);
    }
    let total = sum_money(with_income.iter().map(|row| &row.income))?;
    let top = with_income
        .iter()
        .max_by_key(|row| row.income)
        .expect("non-empty");
    let share = percent_of(top.income, total);
    Ok(share
        .filter(|share| *share >= CHARACTER_CONCENTRATION_PERCENT)
        .map(|share| Insight {
            kind: InsightKind::CharacterConcentration,
            side: Some(InsightSide::Income),
            subject: top.character_name.clone(),
            amount: top.income,
            previous: None,
            total: Some(total),
            share_percent: Some(share),
            change_percent: None,
        }))
}

#[cfg(test)]
mod tests;
