//! Throttles user-triggered ESI syncs. Each manual sync refreshes the token
//! and pulls every asset/wallet page for a character; without a gate, a
//! client could run them back to back or concurrently, burning ESI budget the
//! whole app shares. The worker's scheduled character sync doesn't go
//! through here.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use iskworks_core::{ConnectedCharacterId, EsiSyncKind};

/// Minimum time between two manual syncs of the same data for one character.
pub(crate) const MANUAL_SYNC_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum SyncedData {
    Assets,
    Wallet,
}

fn synced_data(kind: EsiSyncKind) -> &'static [SyncedData] {
    match kind {
        EsiSyncKind::Assets => &[SyncedData::Assets],
        EsiSyncKind::WalletTransactions => &[SyncedData::Wallet],
        EsiSyncKind::AllSupported => &[SyncedData::Assets, SyncedData::Wallet],
    }
}

#[derive(Debug, Default)]
pub(crate) struct ManualSyncGate {
    started: Mutex<HashMap<(ConnectedCharacterId, SyncedData), Instant>>,
}

impl ManualSyncGate {
    /// Records a manual sync starting now, or returns how many seconds until
    /// one may start.
    pub(crate) fn try_begin(&self, id: ConnectedCharacterId, kind: EsiSyncKind) -> Result<(), u64> {
        self.try_begin_at(id, kind, Instant::now())
    }

    fn try_begin_at(
        &self,
        id: ConnectedCharacterId,
        kind: EsiSyncKind,
        now: Instant,
    ) -> Result<(), u64> {
        let mut started = self.started.lock().expect("manual sync gate lock");
        started.retain(|_, at| now.duration_since(*at) < MANUAL_SYNC_INTERVAL);
        let wait = synced_data(kind)
            .iter()
            .filter_map(|data| started.get(&(id, *data)))
            .map(|at| MANUAL_SYNC_INTERVAL - now.duration_since(*at))
            .max();
        if let Some(wait) = wait {
            return Err(wait.as_secs().max(1));
        }
        for data in synced_data(kind) {
            started.insert((id, *data), now);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_sync_of_the_same_data_waits_out_the_interval() {
        let gate = ManualSyncGate::default();
        let id = ConnectedCharacterId::new();
        let now = Instant::now();
        assert!(gate.try_begin_at(id, EsiSyncKind::Assets, now).is_ok());
        assert_eq!(
            gate.try_begin_at(id, EsiSyncKind::Assets, now + Duration::from_secs(20)),
            Err(40)
        );
        assert!(gate
            .try_begin_at(id, EsiSyncKind::Assets, now + MANUAL_SYNC_INTERVAL)
            .is_ok());
    }

    #[test]
    fn different_data_and_different_characters_are_independent() {
        let gate = ManualSyncGate::default();
        let id = ConnectedCharacterId::new();
        let now = Instant::now();
        assert!(gate.try_begin_at(id, EsiSyncKind::Assets, now).is_ok());
        assert!(gate
            .try_begin_at(id, EsiSyncKind::WalletTransactions, now)
            .is_ok());
        assert!(gate
            .try_begin_at(ConnectedCharacterId::new(), EsiSyncKind::Assets, now)
            .is_ok());
    }

    #[test]
    fn sync_all_overlaps_both_kinds() {
        let gate = ManualSyncGate::default();
        let id = ConnectedCharacterId::new();
        let now = Instant::now();
        assert!(gate
            .try_begin_at(id, EsiSyncKind::WalletTransactions, now)
            .is_ok());
        assert!(gate
            .try_begin_at(id, EsiSyncKind::AllSupported, now + Duration::from_secs(1))
            .is_err());
        let later = now + MANUAL_SYNC_INTERVAL;
        assert!(gate
            .try_begin_at(id, EsiSyncKind::AllSupported, later)
            .is_ok());
        assert!(gate.try_begin_at(id, EsiSyncKind::Assets, later).is_err());
    }
}
