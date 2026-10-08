//! Shared inventory fake.

use std::sync::Mutex;

use async_trait::async_trait;
use iskworks_core::{
    InventoryBalance, InventoryError, InventoryEventId, InventoryHistory, InventoryItemKey,
    InventoryPosting, InventoryRepository, Money, OwnerId, WorkspaceId,
};

/// An `InventoryRepository` with no data: reads come back empty or
/// `NotFound`, and every write fails. Wired in by suites that need the
/// inventory dependency present but never exercise it.
pub struct EmptyInventoryRepository;

/// An `InventoryRepository` seeded with a fixed `type_id -> quantity`
/// balance list, counting every `list_balances` call. Any write panics --
/// the Materials projection must never mutate inventory.
pub struct SeededInventoryRepository {
    balances: Vec<(i64, u64)>,
    /// `type_id -> "unit_basis"` string -- when set, that type's balance
    /// reports `average_unit_cost` / `total_historical_cost` instead of the
    /// zero-cost default.
    costs: std::collections::BTreeMap<i64, String>,
    /// `type_id -> quantity` held by active reservations (open Epics).
    reserved: std::collections::BTreeMap<i64, u64>,
    pub list_balances_calls: Mutex<u32>,
}

impl SeededInventoryRepository {
    #[must_use]
    pub fn new(balances: impl IntoIterator<Item = (i64, u64)>) -> Self {
        Self {
            balances: balances.into_iter().collect(),
            costs: std::collections::BTreeMap::new(),
            reserved: std::collections::BTreeMap::new(),
            list_balances_calls: Mutex::new(0),
        }
    }

    /// Attach a weighted-average unit basis (as a decimal string) to a
    /// seeded type -- for verification-export inventory-basis assertions.
    #[must_use]
    pub fn with_unit_basis(mut self, type_id: i64, unit_basis: &str) -> Self {
        self.costs.insert(type_id, unit_basis.to_string());
        self
    }

    /// Mark `quantity` of a seeded type as reserved by open Epics.
    #[must_use]
    pub fn with_reserved(mut self, type_id: i64, quantity: u64) -> Self {
        self.reserved.insert(type_id, quantity);
        self
    }

    #[must_use]
    pub fn list_balances_call_count(&self) -> u32 {
        *self.list_balances_calls.lock().unwrap()
    }
}

#[async_trait]
impl InventoryRepository for SeededInventoryRepository {
    async fn list_balances(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        *self.list_balances_calls.lock().unwrap() += 1;
        Ok(self
            .balances
            .iter()
            .map(|&(type_id, quantity)| {
                let (total, unit) = match self.costs.get(&type_id) {
                    Some(basis) => {
                        let unit = Money::parse(basis).unwrap();
                        (
                            unit.checked_mul_quantity(quantity).unwrap_or(Money::zero()),
                            Some(unit),
                        )
                    }
                    None => (Money::zero(), None),
                };
                InventoryBalance {
                    key: InventoryItemKey {
                        workspace_id,
                        owner_id,
                        type_id,
                    },
                    type_name: format!("T{type_id}"),
                    quantity,
                    total_historical_cost: total,
                    average_unit_cost: unit,
                    revision: 1,
                    last_activity_at: None,
                }
            })
            .collect())
    }

    async fn active_reservations(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<std::collections::BTreeMap<i64, u64>, InventoryError> {
        Ok(self.reserved.clone())
    }

    async fn get_history(
        &self,
        _key: &InventoryItemKey,
    ) -> Result<InventoryHistory, InventoryError> {
        Err(InventoryError::ItemNotFound)
    }

    async fn post(&self, _posting: InventoryPosting) -> Result<InventoryHistory, InventoryError> {
        panic!("the Materials projection must never post an inventory event");
    }

    async fn reverse_latest(
        &self,
        _key: &InventoryItemKey,
        _event_id: InventoryEventId,
        _expected_revision: u64,
        _reason: String,
    ) -> Result<InventoryHistory, InventoryError> {
        panic!("the Materials projection must never reverse an inventory event");
    }

    async fn rebuild(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        panic!("the Materials projection must never rebuild inventory");
    }
}

#[async_trait]
impl InventoryRepository for EmptyInventoryRepository {
    async fn list_balances(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        Ok(Vec::new())
    }

    async fn get_history(
        &self,
        _key: &InventoryItemKey,
    ) -> Result<InventoryHistory, InventoryError> {
        Err(InventoryError::ItemNotFound)
    }

    async fn post(&self, _posting: InventoryPosting) -> Result<InventoryHistory, InventoryError> {
        Err(InventoryError::RevisionConflict)
    }

    async fn reverse_latest(
        &self,
        _key: &InventoryItemKey,
        _event_id: InventoryEventId,
        _expected_revision: u64,
        _reason: String,
    ) -> Result<InventoryHistory, InventoryError> {
        Err(InventoryError::EventNotFound)
    }

    async fn rebuild(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        Ok(Vec::new())
    }
}
