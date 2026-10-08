use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{Money, OwnerId, WorkspaceId};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InventoryEventId(pub Uuid);

impl InventoryEventId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for InventoryEventId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InventoryEventKind {
    OpeningBalance,
    Purchase,
    Consumption,
    ProductionOutput,
    Reversal,
    /// A quantity correction not tied to a purchase, a Build/Order/Ticket
    /// consuming or producing material, or an undo of a prior event --
    /// e.g. a manual "I counted this in my hangar" correction. Covers both
    /// directions: see `adjustment_posting`.
    Adjustment,
}

/// How sure we are of a posting's cost. There is deliberately no `Unknown`
/// variant: every positive accounted quantity must have a defined cost
/// basis (`quantity > 0 => average_unit_cost` is always resolvable) --
/// `ZeroCost` is the only way to record "this is genuinely worth nothing,"
/// and it must be explicitly chosen/acknowledged, never implied by the
/// absence of a cost.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CostInputQuality {
    Known,
    Estimated,
    ZeroCost,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MoneyDelta(#[serde(with = "rust_decimal::serde::str")] pub Decimal);

impl MoneyDelta {
    #[must_use]
    pub fn zero() -> Self {
        let mut value = Decimal::ZERO;
        value.rescale(4);
        Self(value)
    }

    fn from_money(value: Money) -> Self {
        Self(value.0)
    }

    fn checked_neg(self) -> Result<Self, InventoryError> {
        Ok(Self(-self.0))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItemKey {
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    pub type_id: i64,
}

/// `quantity` + `total_historical_cost` is the whole model -- no
/// known/unknown quantity split. `average_unit_cost` is derived
/// (`total_historical_cost / quantity`) and is always `Some` whenever
/// `quantity > 0`; it's `None` only for an empty balance, where there is
/// nothing to divide.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryBalance {
    pub key: InventoryItemKey,
    pub type_name: String,
    pub quantity: u64,
    pub total_historical_cost: Money,
    pub average_unit_cost: Option<Money>,
    pub revision: u64,
    pub last_activity_at: Option<DateTime<Utc>>,
}

impl InventoryBalance {
    #[must_use]
    pub fn empty(key: InventoryItemKey, type_name: String) -> Self {
        Self {
            key,
            type_name,
            quantity: 0,
            total_historical_cost: Money::zero(),
            average_unit_cost: None,
            revision: 0,
            last_activity_at: None,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryEvent {
    pub id: InventoryEventId,
    pub key: InventoryItemKey,
    pub type_name: String,
    pub kind: InventoryEventKind,
    pub quantity_delta: i64,
    pub total_cost_delta: MoneyDelta,
    pub unit_cost: Option<Money>,
    pub cost_quality: CostInputQuality,
    pub source_reference: String,
    pub note: String,
    pub effective_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub sequence: u64,
    pub reverses_event_id: Option<InventoryEventId>,
    pub reversed_by_event_id: Option<InventoryEventId>,
    pub resulting_balance: InventoryBalance,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryHistory {
    pub balance: InventoryBalance,
    pub events: Vec<InventoryEvent>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryPreview {
    pub current: InventoryBalance,
    pub posting: InventoryPosting,
    pub resulting: InventoryBalance,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryPosting {
    pub id: InventoryEventId,
    pub key: InventoryItemKey,
    pub type_name: String,
    pub kind: InventoryEventKind,
    pub quantity_delta: i64,
    pub total_cost_delta: MoneyDelta,
    pub unit_cost: Option<Money>,
    pub cost_quality: CostInputQuality,
    pub source_reference: String,
    pub note: String,
    pub effective_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub expected_revision: u64,
    pub reverses_event_id: Option<InventoryEventId>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostInventoryCommand {
    pub type_id: i64,
    pub type_name: String,
    pub quantity: u64,
    pub unit_cost: Option<String>,
    pub cost_quality: CostInputQuality,
    #[serde(default)]
    pub source_reference: String,
    #[serde(default)]
    pub note: String,
    pub effective_at: DateTime<Utc>,
    pub expected_revision: u64,
    #[serde(default)]
    pub acknowledge_zero_cost: bool,
}

/// Command shape for `InventoryService::preview_adjustment`/`post_adjustment`.
/// Unlike `PostInventoryCommand`, quantity is a signed delta (positive =
/// add, negative = remove) and there is no `cost_quality` -- Adjustment
/// always posts `Known` once a cost is resolved (see `adjustment_posting`).
/// There is also no `effective_at`: an adjustment records a correction
/// discovered right now, not a backdated transaction, so `recorded_at` and
/// `effective_at` are always the same instant (see `adjustment_posting`'s
/// single `recorded_at` parameter).
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostAdjustmentCommand {
    pub type_id: i64,
    pub type_name: String,
    pub quantity_delta: i64,
    pub unit_cost: Option<String>,
    #[serde(default)]
    pub source_reference: String,
    #[serde(default)]
    pub note: String,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseInventoryCommand {
    pub expected_revision: u64,
    pub reason: String,
}

#[async_trait]
pub trait InventoryRepository: Send + Sync {
    async fn list_balances(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError>;
    async fn get_history(&self, key: &InventoryItemKey)
        -> Result<InventoryHistory, InventoryError>;
    /// Every event (in `sequence` order) of each listed type for this
    /// owner, keyed by `type_id` -- the batched sibling of `get_history`'s
    /// `events` for the Inventory list, so the list reads history once per
    /// request instead of once per row. A type with no events is absent.
    /// Defaults to one `get_history` per type.
    async fn list_events_by_type(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, Vec<InventoryEvent>>, InventoryError> {
        let mut events = std::collections::BTreeMap::new();
        for &type_id in type_ids {
            let history = self
                .get_history(&InventoryItemKey {
                    workspace_id,
                    owner_id,
                    type_id,
                })
                .await?;
            if !history.events.is_empty() {
                events.insert(type_id, history.events);
            }
        }
        Ok(events)
    }
    async fn post(&self, posting: InventoryPosting) -> Result<InventoryHistory, InventoryError>;
    async fn reverse_latest(
        &self,
        key: &InventoryItemKey,
        event_id: InventoryEventId,
        expected_revision: u64,
        reason: String,
    ) -> Result<InventoryHistory, InventoryError>;
    async fn rebuild(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError>;
}

#[derive(Clone)]
pub struct InventoryService {
    repository: Arc<dyn InventoryRepository>,
}

impl InventoryService {
    #[must_use]
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn preview_opening(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PostInventoryCommand,
    ) -> Result<InventoryPreview, InventoryError> {
        self.preview(
            workspace_id,
            owner_id,
            InventoryEventKind::OpeningBalance,
            command,
        )
        .await
    }

    pub async fn preview_purchase(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PostInventoryCommand,
    ) -> Result<InventoryPreview, InventoryError> {
        self.preview(
            workspace_id,
            owner_id,
            InventoryEventKind::Purchase,
            command,
        )
        .await
    }

    pub async fn post_opening(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PostInventoryCommand,
    ) -> Result<InventoryHistory, InventoryError> {
        let preview = self
            .preview_opening(workspace_id, owner_id, command)
            .await?;
        self.repository.post(preview.posting).await
    }

    pub async fn post_purchase(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PostInventoryCommand,
    ) -> Result<InventoryHistory, InventoryError> {
        let preview = self
            .preview_purchase(workspace_id, owner_id, command)
            .await?;
        self.repository.post(preview.posting).await
    }

    /// Mirrors `preview_purchase`/`post_purchase` exactly, just tagged
    /// `ProductionOutput` -- used to record a completed Build/Order's
    /// output at its (estimated or actual) production cost.
    pub async fn preview_production_output(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PostInventoryCommand,
    ) -> Result<InventoryPreview, InventoryError> {
        self.preview(
            workspace_id,
            owner_id,
            InventoryEventKind::ProductionOutput,
            command,
        )
        .await
    }

    /// Unlike `preview_opening`/`preview_purchase`/`preview_production_output`
    /// (which build their posting via `create_posting` before ever touching
    /// the balance), `adjustment_posting` needs the *current* balance up
    /// front -- to default a positive adjustment's cost to the existing
    /// weighted average, and to price a negative adjustment's drawdown --
    /// so this fetches first rather than delegating to the shared
    /// `preview` helper.
    pub async fn preview_adjustment(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PostAdjustmentCommand,
    ) -> Result<InventoryPreview, InventoryError> {
        if command.type_id <= 0 || command.type_name.trim().is_empty() {
            return Err(InventoryError::Validation(
                "An EVE item is required.".to_string(),
            ));
        }
        let key = InventoryItemKey {
            workspace_id,
            owner_id,
            type_id: command.type_id,
        };
        let type_name = command.type_name.trim().to_string();
        let current = match self.repository.get_history(&key).await {
            Ok(history) => history.balance,
            Err(InventoryError::ItemNotFound) => {
                InventoryBalance::empty(key.clone(), type_name.clone())
            }
            Err(error) => return Err(error),
        };
        if current.revision != command.expected_revision {
            return Err(InventoryError::RevisionConflict);
        }
        let unit_cost = command
            .unit_cost
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(parse_money)
            .transpose()?;
        let now = Utc::now();
        let posting = adjustment_posting(
            &current,
            command.quantity_delta,
            type_name,
            unit_cost,
            command.source_reference.trim().to_string(),
            command.note.trim().to_string(),
            now,
        )?;
        let resulting = apply_inventory_event(&current, &posting)?;
        Ok(InventoryPreview {
            current,
            posting,
            resulting,
            warnings: Vec::new(),
        })
    }

    pub async fn post_adjustment(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PostAdjustmentCommand,
    ) -> Result<InventoryHistory, InventoryError> {
        let preview = self
            .preview_adjustment(workspace_id, owner_id, command)
            .await?;
        self.repository.post(preview.posting).await
    }

    async fn preview(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        kind: InventoryEventKind,
        command: PostInventoryCommand,
    ) -> Result<InventoryPreview, InventoryError> {
        let posting = create_posting(workspace_id, owner_id, kind, command)?;
        let (current, opening_exists) = match self.repository.get_history(&posting.key).await {
            Ok(history) => (
                history.balance,
                history.events.iter().any(|event| {
                    event.kind == InventoryEventKind::OpeningBalance
                        && event.reversed_by_event_id.is_none()
                }),
            ),
            Err(InventoryError::ItemNotFound) => (
                InventoryBalance::empty(posting.key.clone(), posting.type_name.clone()),
                false,
            ),
            Err(error) => return Err(error),
        };
        if current.revision != posting.expected_revision {
            return Err(InventoryError::RevisionConflict);
        }
        if kind == InventoryEventKind::OpeningBalance && (opening_exists || current.quantity != 0) {
            return Err(InventoryError::OpeningBalanceAlreadyExists);
        }
        let resulting = apply_inventory_event(&current, &posting)?;
        let warnings = warnings_for(posting.cost_quality);
        Ok(InventoryPreview {
            current,
            posting,
            resulting,
            warnings,
        })
    }
}

pub fn apply_inventory_event(
    current: &InventoryBalance,
    posting: &InventoryPosting,
) -> Result<InventoryBalance, InventoryError> {
    if current.key != posting.key {
        return Err(InventoryError::IdentityMismatch);
    }
    let quantity = apply_quantity(current.quantity, posting.quantity_delta)?;
    let total = current
        .total_historical_cost
        .0
        .checked_add(posting.total_cost_delta.0)
        .ok_or(InventoryError::ArithmeticOverflow)?;
    if total.is_sign_negative() || (quantity == 0 && !total.is_zero()) {
        return Err(InventoryError::InvalidProjection);
    }
    let mut total = total;
    total.rescale(4);
    let average_unit_cost = if quantity > 0 {
        let divisor = Decimal::from_i128_with_scale(i128::from(quantity), 0);
        let mut average = total
            .checked_div(divisor)
            .ok_or(InventoryError::ArithmeticOverflow)?;
        average.rescale(4);
        Some(Money(average))
    } else {
        None
    };
    Ok(InventoryBalance {
        key: current.key.clone(),
        type_name: posting.type_name.clone(),
        quantity,
        total_historical_cost: Money(total),
        average_unit_cost,
        revision: current
            .revision
            .checked_add(1)
            .ok_or(InventoryError::ArithmeticOverflow)?,
        last_activity_at: Some(posting.recorded_at),
    })
}

#[cfg(test)]
pub fn rebuild_inventory_balance(
    key: InventoryItemKey,
    type_name: String,
    postings: &[InventoryPosting],
) -> Result<InventoryBalance, InventoryError> {
    let mut balance = InventoryBalance::empty(key, type_name);
    for posting in postings {
        balance = apply_inventory_event(&balance, posting)?;
    }
    Ok(balance)
}

/// The generic purchase primitive: turns authoritative inputs (owner, type,
/// quantity, unit cost, source reference) into a ledger `Purchase` posting.
/// `InventoryService::preview_purchase` and server-side wallet-transaction
/// recording both build postings here, so cost validation and the
/// `unit_cost × quantity` basis math exist exactly once.
pub fn purchase_posting(
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    command: PostInventoryCommand,
) -> Result<InventoryPosting, InventoryError> {
    create_posting(
        workspace_id,
        owner_id,
        InventoryEventKind::Purchase,
        command,
    )
}

fn create_posting(
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    kind: InventoryEventKind,
    command: PostInventoryCommand,
) -> Result<InventoryPosting, InventoryError> {
    if command.type_id <= 0 || command.type_name.trim().is_empty() || command.quantity == 0 {
        return Err(InventoryError::Validation(
            "An EVE item and positive whole-number quantity are required.".to_string(),
        ));
    }
    if command.quantity > i64::MAX as u64 {
        return Err(InventoryError::ArithmeticOverflow);
    }
    if kind == InventoryEventKind::Purchase
        && !matches!(
            command.cost_quality,
            CostInputQuality::Known | CostInputQuality::ZeroCost
        )
    {
        return Err(InventoryError::Validation(
            "Purchases require a known or explicit zero cost.".to_string(),
        ));
    }
    if command.cost_quality == CostInputQuality::ZeroCost && !command.acknowledge_zero_cost {
        return Err(InventoryError::AcknowledgementRequired("zeroCost"));
    }
    let unit_cost = match command.cost_quality {
        CostInputQuality::ZeroCost => Money::zero(),
        CostInputQuality::Known | CostInputQuality::Estimated => {
            parse_money(command.unit_cost.as_deref().unwrap_or(""))?
        }
    };
    let total_cost_delta = MoneyDelta::from_money(
        unit_cost
            .checked_mul_quantity(command.quantity)
            .map_err(map_industry_error)?,
    );
    let quantity_delta =
        i64::try_from(command.quantity).map_err(|_| InventoryError::ArithmeticOverflow)?;
    let now = Utc::now();
    Ok(InventoryPosting {
        id: InventoryEventId::new(),
        key: InventoryItemKey {
            workspace_id,
            owner_id,
            type_id: command.type_id,
        },
        type_name: command.type_name.trim().to_string(),
        kind,
        quantity_delta,
        total_cost_delta,
        unit_cost: Some(unit_cost),
        cost_quality: command.cost_quality,
        source_reference: command.source_reference.trim().to_string(),
        note: command.note.trim().to_string(),
        effective_at: command.effective_at,
        recorded_at: now,
        expected_revision: command.expected_revision,
        reverses_event_id: None,
    })
}

/// Prices a quantity leaving inventory at the balance's current weighted
/// average -- shared by `consumption_posting` and `adjustment_posting`'s
/// negative direction, which price removals identically since there's
/// only one cost pool to draw from. `quantity` must already be validated
/// as `> 0` and `<= current.quantity` by the caller; under the
/// `quantity > 0 => average_unit_cost resolvable` invariant that makes
/// `current.average_unit_cost` unwrappable, but this still returns a
/// (never-expected-in-practice) error instead of panicking if that
/// invariant is ever violated.
fn priced_reduction(
    current: &InventoryBalance,
    quantity: u64,
) -> Result<(MoneyDelta, Money), InventoryError> {
    let average = current
        .average_unit_cost
        .ok_or(InventoryError::InvalidProjection)?;
    let consumed_cost = if quantity == current.quantity {
        current.total_historical_cost
    } else {
        average
            .checked_mul_quantity(quantity)
            .map_err(map_industry_error)?
    };
    // `Decimal` preserves sign on zero, so negating an exactly-zero cost
    // (a zero-cost balance being fully or partially drawn down) would
    // otherwise produce a "-0.0000" that `apply_inventory_event` correctly
    // rejects as inconsistent (`is_sign_negative()` is true even though
    // the value is numerically zero) -- guard explicitly rather than let a
    // real zero-cost reduction look invalid.
    let total_cost_delta = if consumed_cost.0.is_zero() {
        MoneyDelta::zero()
    } else {
        MoneyDelta(-consumed_cost.0)
    };
    Ok((total_cost_delta, average))
}

#[cfg(test)]
pub fn consumption_posting(
    current: &InventoryBalance,
    quantity: u64,
    type_name: String,
    source_reference: String,
    note: String,
    cost_quality: CostInputQuality,
    recorded_at: DateTime<Utc>,
) -> Result<InventoryPosting, InventoryError> {
    if quantity == 0 || quantity > current.quantity {
        return Err(InventoryError::Validation(
            "Consumption requires positive, in-stock inventory.".to_string(),
        ));
    }
    let quantity_delta = i64::try_from(quantity).map_err(|_| InventoryError::ArithmeticOverflow)?;
    let (total_cost_delta, average) = priced_reduction(current, quantity)?;
    Ok(InventoryPosting {
        id: InventoryEventId::new(),
        key: current.key.clone(),
        type_name,
        kind: InventoryEventKind::Consumption,
        quantity_delta: -quantity_delta,
        total_cost_delta,
        unit_cost: Some(average),
        cost_quality,
        source_reference,
        note,
        effective_at: recorded_at,
        recorded_at,
        expected_revision: current.revision,
        reverses_event_id: None,
    })
}

/// Builds an `Adjustment` posting for either direction. The caller is
/// responsible for validating `unit_cost` (non-negative, correctly scaled)
/// before calling, same as any other pre-built `Money` value flowing
/// through this module -- this function operates on already-trusted
/// domain values, not raw command strings.
///
/// - **Positive** `quantity_delta`: an explicit `unit_cost` is used
///   directly; if omitted, defaults to the balance's current weighted
///   average (`current.average_unit_cost`) -- if there is no existing
///   cost basis to default from (a brand-new item, `average_unit_cost ==
///   None`), the adjustment is rejected rather than silently posting a
///   zero or unknown cost. Always tagged `Known`.
/// - **Negative** `quantity_delta`: `unit_cost` must be `None` -- the
///   caller is asserting how much physical stock is gone, not pricing the
///   loss. Reduces quantity and carrying cost at the balance's current
///   weighted average, identical to `consumption_posting`'s math
///   (`priced_reduction`), including the exact-total shortcut when the
///   adjustment removes the entire balance (avoids rounding drift). A
///   negative delta larger than the entire physical balance is rejected
///   before any cost math runs.
pub fn adjustment_posting(
    current: &InventoryBalance,
    quantity_delta: i64,
    type_name: String,
    unit_cost: Option<Money>,
    source_reference: String,
    note: String,
    recorded_at: DateTime<Utc>,
) -> Result<InventoryPosting, InventoryError> {
    if quantity_delta == 0 {
        return Err(InventoryError::Validation(
            "An adjustment must have a nonzero quantity.".to_string(),
        ));
    }
    let (total_cost_delta, unit_cost) = if quantity_delta > 0 {
        let quantity = quantity_delta.unsigned_abs();
        let cost = match unit_cost {
            Some(cost) => cost,
            None => current.average_unit_cost.ok_or_else(|| {
                InventoryError::Validation(
                    "A unit cost is required: this item has no existing cost basis to default from."
                        .to_string(),
                )
            })?,
        };
        let total = cost
            .checked_mul_quantity(quantity)
            .map_err(map_industry_error)?;
        (MoneyDelta::from_money(total), Some(cost))
    } else {
        if unit_cost.is_some() {
            return Err(InventoryError::Validation(
                "A negative adjustment cannot assert a unit cost.".to_string(),
            ));
        }
        let magnitude = quantity_delta.unsigned_abs();
        if magnitude > current.quantity {
            return Err(InventoryError::NegativeBalance);
        }
        let (total_cost_delta, average) = priced_reduction(current, magnitude)?;
        (total_cost_delta, Some(average))
    };
    Ok(InventoryPosting {
        id: InventoryEventId::new(),
        key: current.key.clone(),
        type_name,
        kind: InventoryEventKind::Adjustment,
        quantity_delta,
        total_cost_delta,
        unit_cost,
        cost_quality: CostInputQuality::Known,
        source_reference,
        note,
        effective_at: recorded_at,
        recorded_at,
        expected_revision: current.revision,
        reverses_event_id: None,
    })
}

fn parse_money(value: &str) -> Result<Money, InventoryError> {
    let mut decimal = Decimal::from_str(value.trim()).map_err(|_| InventoryError::InvalidMoney)?;
    if decimal.is_sign_negative() || decimal.normalize().scale() > 4 {
        return Err(InventoryError::InvalidMoney);
    }
    decimal.rescale(4);
    Ok(Money(decimal))
}

fn map_industry_error(_: crate::IndustryError) -> InventoryError {
    InventoryError::ArithmeticOverflow
}

fn apply_quantity(current: u64, delta: i64) -> Result<u64, InventoryError> {
    if delta >= 0 {
        current
            .checked_add(delta as u64)
            .ok_or(InventoryError::ArithmeticOverflow)
    } else {
        current
            .checked_sub(delta.unsigned_abs())
            .ok_or(InventoryError::NegativeBalance)
    }
}

fn warnings_for(quality: CostInputQuality) -> Vec<String> {
    let mut warnings = Vec::new();
    if quality == CostInputQuality::Estimated {
        warnings.push(
            "This was recorded at an estimated cost because no actual cost was entered."
                .to_string(),
        );
    }
    if quality == CostInputQuality::ZeroCost {
        warnings.push("This inventory was explicitly recorded at zero cost. Future accounting profit may appear unusually high.".to_string());
    }
    warnings
}

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("money must be a non-negative decimal with at most four fractional digits")]
    InvalidMoney,
    #[error("acknowledgement is required for {0}")]
    AcknowledgementRequired(&'static str),
    #[error("inventory item was not found")]
    ItemNotFound,
    #[error("inventory event was not found")]
    EventNotFound,
    #[error("an opening balance already exists for this inventory item")]
    OpeningBalanceAlreadyExists,
    #[error("inventory changed while this form was open")]
    RevisionConflict,
    #[error("only the latest unreversed event can be reversed")]
    LatestEventOnly,
    #[error("this event was already reversed")]
    AlreadyReversed,
    #[error(
        "materials consumed by a Build cannot be reversed; Build cancellation is not supported yet"
    )]
    BuildConsumptionNotReversible,
    #[error(
        "manufactured output created by a Completed Build cannot be reversed; Build reversal is not supported yet"
    )]
    BuildOutputNotReversible,
    #[error("reversal would create a negative balance")]
    NegativeBalance,
    #[error("inventory identity does not match")]
    IdentityMismatch,
    #[error("inventory projection is inconsistent")]
    InvalidProjection,
    #[error("inventory arithmetic exceeds the supported range")]
    ArithmeticOverflow,
    #[error("persistence failed: {0}")]
    Persistence(String),
}

pub fn reversal_posting(
    original: &InventoryEvent,
    expected_revision: u64,
    reason: String,
) -> Result<InventoryPosting, InventoryError> {
    if original.kind == InventoryEventKind::Consumption {
        return Err(InventoryError::BuildConsumptionNotReversible);
    }
    if original.kind == InventoryEventKind::ProductionOutput {
        return Err(InventoryError::BuildOutputNotReversible);
    }
    if original.kind == InventoryEventKind::Reversal {
        return Err(InventoryError::LatestEventOnly);
    }
    if original.reversed_by_event_id.is_some() {
        return Err(InventoryError::AlreadyReversed);
    }
    let now = Utc::now();
    Ok(InventoryPosting {
        id: InventoryEventId::new(),
        key: original.key.clone(),
        type_name: original.type_name.clone(),
        kind: InventoryEventKind::Reversal,
        quantity_delta: original
            .quantity_delta
            .checked_neg()
            .ok_or(InventoryError::ArithmeticOverflow)?,
        total_cost_delta: original.total_cost_delta.checked_neg()?,
        unit_cost: original.unit_cost,
        cost_quality: original.cost_quality,
        source_reference: String::new(),
        note: reason.trim().to_string(),
        effective_at: now,
        recorded_at: now,
        expected_revision,
        reverses_event_id: Some(original.id),
    })
}

#[cfg(test)]
mod tests;
