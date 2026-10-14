//! Capture for a recording preview: `run_ticket_acquisition` /
//! `run_ticket_production` record into their transaction as usual, and this
//! reads the touched balances before and after so the transaction can be
//! rolled back -- the preview is exactly what recording would post.

use iskworks_core::InventoryBalance;

use super::*;

#[derive(Default)]
pub(super) struct PreviewCapture {
    items: Vec<(InventoryItemKey, String, RecordingPreviewEffect)>,
    before: Vec<InventoryBalance>,
    lines: Vec<RecordingPreviewLine>,
}

impl PreviewCapture {
    pub(super) async fn read_before(
        &mut self,
        tx: &mut Transaction<'_, Postgres>,
        items: Vec<(InventoryItemKey, String, RecordingPreviewEffect)>,
    ) -> Result<(), OrderError> {
        for (key, type_name, _) in &items {
            self.before
                .push(balance_or_empty(tx, key, type_name).await?);
        }
        self.items = items;
        Ok(())
    }

    pub(super) async fn read_after(
        &mut self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<(), OrderError> {
        for ((key, type_name, effect), current) in self.items.iter().zip(self.before.drain(..)) {
            let resulting = balance_or_empty(tx, key, type_name).await?;
            self.lines.push(RecordingPreviewLine {
                type_id: key.type_id,
                type_name: type_name.clone(),
                effect: *effect,
                current,
                resulting,
            });
        }
        Ok(())
    }

    pub(super) fn into_preview(self) -> RecordingPreview {
        RecordingPreview { lines: self.lines }
    }
}

async fn balance_or_empty(
    tx: &mut Transaction<'_, Postgres>,
    key: &InventoryItemKey,
    type_name: &str,
) -> Result<InventoryBalance, OrderError> {
    Ok(PgInventoryRepository::balance_with(&mut **tx, key)
        .await
        .map_err(|error| OrderError::Persistence(error.to_string()))?
        .unwrap_or_else(|| InventoryBalance::empty(key.clone(), type_name.to_string())))
}
