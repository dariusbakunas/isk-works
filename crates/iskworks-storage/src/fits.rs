use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::fitting::{
    Fit, FitDraft, FitId, FitItem, FitMutation, FitRepository, ModuleState, SlotKind,
};
use iskworks_core::{InventoryError, WorkspaceId};
use sqlx::types::Json;
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgFitRepository {
    pool: PgPool,
}

impl PgFitRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn persistence(error: impl std::fmt::Display) -> InventoryError {
    InventoryError::Persistence(error.to_string())
}

#[derive(sqlx::FromRow)]
struct FitRow {
    id: Uuid,
    name: String,
    description: String,
    ship_type_id: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    fit_id: Uuid,
    slot_kind: String,
    slot_index: Option<i16>,
    type_id: i64,
    quantity: i32,
    state: String,
    charge_type_id: Option<i64>,
    mutation: Option<Json<FitMutation>>,
}

impl ItemRow {
    fn into_item(self) -> Result<FitItem, InventoryError> {
        Ok(FitItem {
            type_id: self.type_id,
            slot_kind: self.slot_kind.parse::<SlotKind>().map_err(persistence)?,
            slot_index: self.slot_index,
            quantity: u32::try_from(self.quantity).map_err(persistence)?,
            state: self.state.parse::<ModuleState>().map_err(persistence)?,
            charge_type_id: self.charge_type_id,
            mutation: self.mutation.map(|Json(mutation)| mutation),
        })
    }
}

impl PgFitRepository {
    /// The fits' items in saved order and tags by name, attached to their fits.
    async fn with_children(&self, rows: Vec<FitRow>) -> Result<Vec<Fit>, InventoryError> {
        let ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
        let mut items = HashMap::<Uuid, Vec<FitItem>>::new();
        let mut tags = HashMap::<Uuid, Vec<String>>::new();
        if !ids.is_empty() {
            let item_rows = sqlx::query_as::<_, ItemRow>(
                "SELECT fit_id, slot_kind, slot_index, type_id, quantity, state,
                        charge_type_id, mutation
                 FROM fit_items
                 WHERE fit_id = ANY($1)
                 ORDER BY fit_id, position",
            )
            .bind(&ids)
            .fetch_all(&self.pool)
            .await
            .map_err(persistence)?;
            for row in item_rows {
                let fit_id = row.fit_id;
                items.entry(fit_id).or_default().push(row.into_item()?);
            }
            for (fit_id, tag) in fetch_tags(&self.pool, &ids).await? {
                tags.entry(fit_id).or_default().push(tag);
            }
        }
        Ok(rows
            .into_iter()
            .map(|row| Fit {
                items: items.remove(&row.id).unwrap_or_default(),
                tags: tags.remove(&row.id).unwrap_or_default(),
                id: FitId(row.id),
                name: row.name,
                description: row.description,
                ship_type_id: row.ship_type_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect())
    }
}

async fn insert_items(
    tx: &mut Transaction<'_, Postgres>,
    fit_id: Uuid,
    items: &[FitItem],
) -> Result<(), InventoryError> {
    if items.is_empty() {
        return Ok(());
    }
    let mut query = QueryBuilder::<Postgres>::new(
        "INSERT INTO fit_items (fit_id, position, slot_kind, slot_index, type_id, quantity, \
         state, charge_type_id, mutation) ",
    );
    let mut rows = Vec::with_capacity(items.len());
    for (position, item) in items.iter().enumerate() {
        rows.push((
            i32::try_from(position).map_err(persistence)?,
            i32::try_from(item.quantity).map_err(persistence)?,
            item,
        ));
    }
    query.push_values(rows, |mut row, (position, quantity, item)| {
        row.push_bind(fit_id)
            .push_bind(position)
            .push_bind(item.slot_kind.as_str())
            .push_bind(item.slot_index)
            .push_bind(item.type_id)
            .push_bind(quantity)
            .push_bind(item.state.as_str())
            .push_bind(item.charge_type_id)
            .push_bind(item.mutation.clone().map(Json));
    });
    query
        .build()
        .execute(&mut **tx)
        .await
        .map_err(persistence)?;
    Ok(())
}

/// The fits' tags, by name within each fit.
async fn fetch_tags<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, String)>, InventoryError> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT fit_id, tag FROM fit_tags WHERE fit_id = ANY($1) ORDER BY fit_id, lower(tag), tag",
    )
    .bind(ids)
    .fetch_all(executor)
    .await
    .map_err(persistence)
}

const FIT_COLUMNS: &str = "id, name, description, ship_type_id, created_at, updated_at";

#[async_trait]
impl FitRepository for PgFitRepository {
    async fn list_fits(&self, workspace_id: WorkspaceId) -> Result<Vec<Fit>, InventoryError> {
        let rows = sqlx::query_as::<_, FitRow>(&format!(
            "SELECT {FIT_COLUMNS} FROM fits
             WHERE workspace_id = $1
             ORDER BY lower(name), id"
        ))
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)?;
        self.with_children(rows).await
    }

    async fn fit(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
    ) -> Result<Option<Fit>, InventoryError> {
        let row = sqlx::query_as::<_, FitRow>(&format!(
            "SELECT {FIT_COLUMNS} FROM fits WHERE workspace_id = $1 AND id = $2"
        ))
        .bind(workspace_id.0)
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(persistence)?;
        Ok(self.with_children(row.into_iter().collect()).await?.pop())
    }

    async fn create_fit(
        &self,
        workspace_id: WorkspaceId,
        draft: &FitDraft,
        now: DateTime<Utc>,
    ) -> Result<Fit, InventoryError> {
        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        let row = sqlx::query_as::<_, FitRow>(&format!(
            "INSERT INTO fits (id, workspace_id, name, description, ship_type_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $6)
             RETURNING {FIT_COLUMNS}"
        ))
        .bind(id)
        .bind(workspace_id.0)
        .bind(draft.name.trim())
        .bind(&draft.description)
        .bind(draft.ship_type_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(persistence)?;
        insert_items(&mut tx, id, &draft.items).await?;
        tx.commit().await.map_err(persistence)?;
        Ok(Fit {
            id: FitId(row.id),
            name: row.name,
            description: row.description,
            ship_type_id: row.ship_type_id,
            items: draft.items.clone(),
            tags: Vec::new(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    async fn update_fit(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
        draft: &FitDraft,
        now: DateTime<Utc>,
    ) -> Result<Option<Fit>, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        let Some(row) = sqlx::query_as::<_, FitRow>(&format!(
            "UPDATE fits SET name = $3, description = $4, ship_type_id = $5, updated_at = $6
             WHERE workspace_id = $1 AND id = $2
             RETURNING {FIT_COLUMNS}"
        ))
        .bind(workspace_id.0)
        .bind(id.0)
        .bind(draft.name.trim())
        .bind(&draft.description)
        .bind(draft.ship_type_id)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(persistence)?
        else {
            return Ok(None);
        };
        sqlx::query("DELETE FROM fit_items WHERE fit_id = $1")
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;
        insert_items(&mut tx, id.0, &draft.items).await?;
        let tags = fetch_tags(&mut *tx, &[id.0]).await?;
        tx.commit().await.map_err(persistence)?;
        Ok(Some(Fit {
            id,
            name: row.name,
            description: row.description,
            ship_type_id: row.ship_type_id,
            items: draft.items.clone(),
            tags: tags.into_iter().map(|(_, tag)| tag).collect(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }))
    }

    async fn delete_fit(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
    ) -> Result<bool, InventoryError> {
        let deleted = sqlx::query("DELETE FROM fits WHERE workspace_id = $1 AND id = $2")
            .bind(workspace_id.0)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(persistence)?;
        Ok(deleted.rows_affected() > 0)
    }

    async fn fit_tags_in_use(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<String>, InventoryError> {
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT ON (lower(t.tag)) t.tag
             FROM fit_tags t JOIN fits f ON f.id = t.fit_id
             WHERE f.workspace_id = $1
             ORDER BY lower(t.tag), t.tag",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(persistence)
    }

    async fn replace_fit_tags(
        &self,
        workspace_id: WorkspaceId,
        id: FitId,
        tags: &[String],
    ) -> Result<Option<Vec<String>>, InventoryError> {
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        let owned = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM fits WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(workspace_id.0)
        .bind(id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(persistence)?;
        if owned.is_none() {
            return Ok(None);
        }
        sqlx::query("DELETE FROM fit_tags WHERE fit_id = $1")
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;
        sqlx::query(
            "INSERT INTO fit_tags (fit_id, tag)
             SELECT $1, tag FROM unnest($2::text[]) AS tag
             ON CONFLICT DO NOTHING",
        )
        .bind(id.0)
        .bind(tags)
        .execute(&mut *tx)
        .await
        .map_err(persistence)?;
        let stored = fetch_tags(&mut *tx, &[id.0]).await?;
        tx.commit().await.map_err(persistence)?;
        Ok(Some(stored.into_iter().map(|(_, tag)| tag).collect()))
    }

    async fn edit_fit_tags(
        &self,
        workspace_id: WorkspaceId,
        ids: &[FitId],
        add: &[String],
        remove: &[String],
    ) -> Result<(), InventoryError> {
        let ids = ids.iter().map(|id| id.0).collect::<Vec<_>>();
        let removed = remove
            .iter()
            .map(|tag| tag.to_lowercase())
            .collect::<Vec<_>>();
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        sqlx::query(
            "DELETE FROM fit_tags t USING fits f
             WHERE f.id = t.fit_id AND f.workspace_id = $1 AND t.fit_id = ANY($2)
               AND lower(t.tag) = ANY($3)",
        )
        .bind(workspace_id.0)
        .bind(&ids)
        .bind(&removed)
        .execute(&mut *tx)
        .await
        .map_err(persistence)?;
        sqlx::query(
            "INSERT INTO fit_tags (fit_id, tag)
             SELECT f.id, tag FROM fits f CROSS JOIN unnest($3::text[]) AS tag
             WHERE f.workspace_id = $1 AND f.id = ANY($2)
             ON CONFLICT DO NOTHING",
        )
        .bind(workspace_id.0)
        .bind(&ids)
        .bind(add)
        .execute(&mut *tx)
        .await
        .map_err(persistence)?;
        tx.commit().await.map_err(persistence)
    }
}
