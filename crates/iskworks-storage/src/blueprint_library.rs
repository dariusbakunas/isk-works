use async_trait::async_trait;
use chrono::{DateTime, Utc};
use iskworks_core::{
    AssetFilterOption, BlueprintActivity, BlueprintGroupFacet, BlueprintKind,
    BlueprintLibraryBuildRef, BlueprintLibraryFacets, BlueprintLibraryOrder, BlueprintLibraryPage,
    BlueprintLibraryRepository, BlueprintLibraryRow, BlueprintLibrarySort, BlueprintLibrarySummary,
    BuildId, IndustryError, ValidatedBlueprintLibraryQuery, WorkspaceId,
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PgBlueprintLibraryRepository {
    pool: PgPool,
}

impl PgBlueprintLibraryRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Every synced blueprint of a workspace, classified by its product and with
/// its effective location: a blueprint inside a container resolves through
/// the asset hierarchy to the station / structure the container sits in.
/// `$1` is the workspace.
const LIBRARY_CTE: &str = r#"
  active_import AS (SELECT id FROM sde_imports WHERE active LIMIT 1),
  used AS (
    SELECT draft.planning_input #>> '{blueprintSelection,observationId}' AS observation_id,
           array_agg(build.id ORDER BY build.display_name,build.id) AS build_ids,
           array_agg(build.display_name ORDER BY build.display_name,build.id) AS build_names
    FROM build_draft_planning draft
    JOIN builds build ON build.id=draft.build_id
    WHERE build.workspace_id=$1
      AND draft.planning_input #>> '{blueprintSelection,mode}'='observedAsset'
    GROUP BY 1
  ),
  library AS (
    SELECT observation.id AS observation_id,observation.eve_item_id,
           observation.blueprint_type_id,
           COALESCE(blueprint_type.name_en,observation.captured_blueprint_name) AS blueprint_name,
           CASE WHEN formula.reaction_formula_type_id IS NOT NULL
                THEN 'reaction' ELSE 'manufacturing' END AS activity,
           product.product_type_id,product_type.name_en AS product_name,
           product_type.group_id AS product_group_id,
           COALESCE(product_group.name_en,product_type.group_name_en) AS product_group_name,
           product_group.category_id AS product_category_id,
           product_category.name_en AS product_category_name,
           observation.blueprint_kind,
           observation.material_efficiency::integer AS material_efficiency,
           observation.time_efficiency::integer AS time_efficiency,
           observation.licensed_runs,
           observation.connection_id,connection.character_name,
           COALESCE(asset.effective_location_id,observation.location_id) AS location_id,
           COALESCE(location.location_name,station.name_en,system.name_en,
                    observation.captured_location_name) AS location_name,
           observation.location_flag,observation.observed_at,
           used.build_ids AS used_build_ids,used.build_names AS used_build_names
    FROM blueprint_observations observation
    LEFT JOIN active_import import ON true
    LEFT JOIN eve_connections connection ON connection.id=observation.connection_id
    LEFT JOIN sde_types blueprint_type
      ON blueprint_type.import_id=import.id AND blueprint_type.type_id=observation.blueprint_type_id
    LEFT JOIN sde_reaction_formulas formula
      ON formula.import_id=import.id
     AND formula.reaction_formula_type_id=observation.blueprint_type_id
    LEFT JOIN LATERAL (
      SELECT candidate.product_type_id FROM (
        SELECT 0 AS source,product.position,product.product_type_id
        FROM sde_blueprint_products product
        WHERE product.import_id=import.id AND product.blueprint_type_id=observation.blueprint_type_id
        UNION ALL
        SELECT 1,product.position,product.product_type_id
        FROM sde_reaction_formula_products product
        WHERE product.import_id=import.id
          AND product.reaction_formula_type_id=observation.blueprint_type_id
      ) candidate
      ORDER BY candidate.source,candidate.position LIMIT 1
    ) product ON true
    LEFT JOIN sde_types product_type
      ON product_type.import_id=import.id AND product_type.type_id=product.product_type_id
    LEFT JOIN sde_groups product_group
      ON product_group.import_id=import.id AND product_group.group_id=product_type.group_id
    LEFT JOIN sde_categories product_category
      ON product_category.import_id=import.id
     AND product_category.category_id=product_group.category_id
    LEFT JOIN LATERAL (
      SELECT current.effective_location_id,current.effective_location_type
      FROM asset_browser_current current
      WHERE current.connection_id=observation.connection_id
        AND current.source_item_id=observation.eve_item_id
      LIMIT 1
    ) asset ON true
    LEFT JOIN market_location_names location
      ON location.workspace_id=observation.workspace_id
     AND location.location_id=COALESCE(asset.effective_location_id,observation.location_id)
    LEFT JOIN sde_npc_stations station
      ON station.import_id=import.id
     AND station.station_id=COALESCE(asset.effective_location_id,observation.location_id)
    LEFT JOIN sde_solar_systems system
      ON system.import_id=import.id
     AND system.solar_system_id=COALESCE(location.solar_system_id,station.solar_system_id,
       CASE WHEN asset.effective_location_type='solar_system'
            THEN asset.effective_location_id END)
    LEFT JOIN used ON used.observation_id=observation.id::text
    WHERE observation.workspace_id=$1
  )"#;

#[async_trait]
impl BlueprintLibraryRepository for PgBlueprintLibraryRepository {
    async fn list(
        &self,
        workspace_id: WorkspaceId,
        query: &ValidatedBlueprintLibraryQuery,
    ) -> Result<BlueprintLibraryPage, IndustryError> {
        let search = format!("%{}%", escape_like(&query.search));
        let kinds = query
            .kinds
            .iter()
            .map(|kind| kind_text(*kind).to_string())
            .collect::<Vec<_>>();
        let activity = query.activity.map(|activity| match activity {
            BlueprintActivity::Manufacturing => "manufacturing",
            BlueprintActivity::Reaction => "reaction",
        });
        let sort = match query.sort {
            BlueprintLibrarySort::Blueprint => "blueprint",
            BlueprintLibrarySort::Product => "product",
            BlueprintLibrarySort::Kind => "kind",
            BlueprintLibrarySort::Me => "me",
            BlueprintLibrarySort::Te => "te",
            BlueprintLibrarySort::Runs => "runs",
            BlueprintLibrarySort::Character => "character",
            BlueprintLibrarySort::Location => "location",
            BlueprintLibrarySort::Observed => "observed",
        };
        let order = match query.order {
            BlueprintLibraryOrder::Asc => "asc",
            BlueprintLibraryOrder::Desc => "desc",
        };
        let sql = format!(
            r#"WITH {LIBRARY_CTE},
               filtered AS (
                 SELECT *,COUNT(*) OVER()::bigint AS total,
                        CASE WHEN blueprint_kind='original' THEN 9223372036854775807
                             ELSE COALESCE(licensed_runs,0) END AS runs_key
                 FROM library
                 WHERE ($2='' OR blueprint_name ILIKE $3 OR COALESCE(product_name,'') ILIKE $3
                        OR COALESCE(character_name,'') ILIKE $3
                        OR COALESCE(location_name,'') ILIKE $3)
                   AND (cardinality($4::text[])=0 OR blueprint_kind=ANY($4))
                   AND (cardinality($5::bigint[])=0 OR product_category_id=ANY($5))
                   AND (cardinality($6::bigint[])=0 OR product_group_id=ANY($6))
                   AND (cardinality($7::uuid[])=0 OR connection_id=ANY($7))
                   AND (cardinality($8::bigint[])=0 OR location_id=ANY($8))
                   AND ($9::integer IS NULL OR material_efficiency>=$9)
                   AND ($10::integer IS NULL OR time_efficiency>=$10)
                   AND (NOT $11 OR blueprint_kind='original' OR COALESCE(licensed_runs,0)>0)
                   AND ($12::text IS NULL OR activity=$12)
               )
               SELECT * FROM filtered
               ORDER BY
                 CASE WHEN $13='blueprint' AND $14='asc' THEN lower(blueprint_name) END ASC,
                 CASE WHEN $13='blueprint' AND $14='desc' THEN lower(blueprint_name) END DESC,
                 CASE WHEN $13='product' AND $14='asc' THEN lower(product_name) END ASC NULLS LAST,
                 CASE WHEN $13='product' AND $14='desc' THEN lower(product_name) END DESC NULLS LAST,
                 CASE WHEN $13='kind' AND $14='asc' THEN blueprint_kind END ASC,
                 CASE WHEN $13='kind' AND $14='desc' THEN blueprint_kind END DESC,
                 CASE WHEN $13='me' AND $14='asc' THEN material_efficiency END ASC,
                 CASE WHEN $13='me' AND $14='desc' THEN material_efficiency END DESC,
                 CASE WHEN $13='te' AND $14='asc' THEN time_efficiency END ASC,
                 CASE WHEN $13='te' AND $14='desc' THEN time_efficiency END DESC,
                 CASE WHEN $13='runs' AND $14='asc' THEN runs_key END ASC,
                 CASE WHEN $13='runs' AND $14='desc' THEN runs_key END DESC,
                 CASE WHEN $13='character' AND $14='asc' THEN lower(character_name) END ASC NULLS LAST,
                 CASE WHEN $13='character' AND $14='desc' THEN lower(character_name) END DESC NULLS LAST,
                 CASE WHEN $13='location' AND $14='asc' THEN lower(location_name) END ASC NULLS LAST,
                 CASE WHEN $13='location' AND $14='desc' THEN lower(location_name) END DESC NULLS LAST,
                 CASE WHEN $13='observed' AND $14='asc' THEN observed_at END ASC,
                 CASE WHEN $13='observed' AND $14='desc' THEN observed_at END DESC,
                 lower(blueprint_name),connection_id NULLS LAST,eve_item_id,observation_id
               LIMIT $15 OFFSET $16"#
        );
        let rows = sqlx::query_as::<_, LibraryRow>(&sql)
            .bind(workspace_id.0)
            .bind(&query.search)
            .bind(search)
            .bind(&kinds)
            .bind(&query.category_ids)
            .bind(&query.group_ids)
            .bind(&query.connection_ids)
            .bind(&query.location_ids)
            .bind(query.min_me.map(i32::from))
            .bind(query.min_te.map(i32::from))
            .bind(query.usable_only)
            .bind(activity)
            .bind(sort)
            .bind(order)
            .bind(i64::from(query.limit))
            .bind(i64::from(query.offset))
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)?;
        let total = match rows.first() {
            Some(row) => row.total,
            // Past the last page the window count is gone; count directly.
            None if query.offset > 0 => self.count(workspace_id, query).await?,
            None => 0,
        };
        let summary = sqlx::query_as::<_, SummaryRow>(
            r#"SELECT COUNT(*) FILTER (WHERE blueprint_kind='original')::bigint AS originals,
                      COUNT(*) FILTER (WHERE blueprint_kind='copy')::bigint AS copies,
                      COUNT(DISTINCT connection_id)::bigint AS characters,
                      MAX(observed_at) AS latest_observed_at
               FROM blueprint_observations WHERE workspace_id=$1"#,
        )
        .bind(workspace_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)?;
        Ok(BlueprintLibraryPage {
            rows: rows
                .into_iter()
                .map(LibraryRow::into_domain)
                .collect::<Result<_, _>>()?,
            total,
            summary: BlueprintLibrarySummary {
                originals: summary.originals,
                copies: summary.copies,
                characters: summary.characters,
                latest_observed_at: summary.latest_observed_at,
            },
        })
    }

    async fn facets(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<BlueprintLibraryFacets, IndustryError> {
        let sql = format!(
            r#"WITH {LIBRARY_CTE}
               SELECT 'category' AS facet,COALESCE(product_category_id::text,'') AS value,
                      COALESCE(MIN(product_category_name),'Unknown') AS label,
                      COUNT(*)::bigint AS count,NULL::bigint AS category_id
               FROM library GROUP BY product_category_id
               UNION ALL
               SELECT 'group',COALESCE(product_group_id::text,''),
                      COALESCE(MIN(product_group_name),'Unknown'),COUNT(*)::bigint,
                      MIN(product_category_id)
               FROM library GROUP BY product_group_id
               UNION ALL
               SELECT 'character',COALESCE(connection_id::text,''),
                      COALESCE(MIN(character_name),'Unknown character'),COUNT(*)::bigint,NULL
               FROM library GROUP BY connection_id
               UNION ALL
               SELECT 'location',location_id::text,
                      COALESCE(MIN(location_name),'Location '||location_id::text),
                      COUNT(*)::bigint,NULL
               FROM library GROUP BY location_id
               ORDER BY facet,label,value"#
        );
        let rows = sqlx::query_as::<_, FacetRow>(&sql)
            .bind(workspace_id.0)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)?;
        let mut facets = BlueprintLibraryFacets::default();
        for row in rows {
            // An empty value is the "no product / no connection" bucket; it
            // cannot be selected as a filter, so it is not offered.
            if row.value.is_empty() {
                continue;
            }
            let option = AssetFilterOption {
                value: row.value,
                label: row.label,
                count: row.count,
            };
            match row.facet.as_str() {
                "category" => facets.categories.push(option),
                "group" => facets.groups.push(BlueprintGroupFacet {
                    value: option.value,
                    label: option.label,
                    count: option.count,
                    category_id: row.category_id,
                }),
                "character" => facets.characters.push(option),
                _ => facets.locations.push(option),
            }
        }
        Ok(facets)
    }
}

impl PgBlueprintLibraryRepository {
    async fn count(
        &self,
        workspace_id: WorkspaceId,
        query: &ValidatedBlueprintLibraryQuery,
    ) -> Result<i64, IndustryError> {
        let first_page = ValidatedBlueprintLibraryQuery {
            offset: 0,
            limit: 1,
            ..query.clone()
        };
        Ok(self.list(workspace_id, &first_page).await?.total)
    }
}

fn kind_text(kind: BlueprintKind) -> &'static str {
    match kind {
        BlueprintKind::Original => "original",
        BlueprintKind::Copy => "copy",
        BlueprintKind::Unknown => "unknown",
    }
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn map_error(error: sqlx::Error) -> IndustryError {
    crate::pool_metrics::observe_error(&error);
    IndustryError::Persistence(error.to_string())
}

#[derive(sqlx::FromRow)]
struct LibraryRow {
    observation_id: Uuid,
    eve_item_id: i64,
    blueprint_type_id: i64,
    blueprint_name: String,
    activity: String,
    product_type_id: Option<i64>,
    product_name: Option<String>,
    product_group_id: Option<i64>,
    product_group_name: Option<String>,
    product_category_id: Option<i64>,
    product_category_name: Option<String>,
    blueprint_kind: String,
    material_efficiency: i32,
    time_efficiency: i32,
    licensed_runs: Option<i64>,
    connection_id: Option<Uuid>,
    character_name: Option<String>,
    location_id: i64,
    location_name: Option<String>,
    location_flag: String,
    observed_at: DateTime<Utc>,
    used_build_ids: Option<Vec<Uuid>>,
    used_build_names: Option<Vec<String>>,
    total: i64,
}

impl LibraryRow {
    fn into_domain(self) -> Result<BlueprintLibraryRow, IndustryError> {
        let invalid = |what: &str| IndustryError::Persistence(format!("invalid stored {what}"));
        Ok(BlueprintLibraryRow {
            observation_id: self.observation_id,
            eve_item_id: self.eve_item_id,
            blueprint_type_id: self.blueprint_type_id,
            blueprint_name: self.blueprint_name,
            activity: if self.activity == "reaction" {
                BlueprintActivity::Reaction
            } else {
                BlueprintActivity::Manufacturing
            },
            product_type_id: self.product_type_id,
            product_name: self.product_name,
            product_group_id: self.product_group_id,
            product_group_name: self.product_group_name,
            product_category_id: self.product_category_id,
            product_category_name: self.product_category_name,
            kind: match self.blueprint_kind.as_str() {
                "original" => BlueprintKind::Original,
                "copy" => BlueprintKind::Copy,
                _ => BlueprintKind::Unknown,
            },
            material_efficiency: u8::try_from(self.material_efficiency)
                .map_err(|_| invalid("material efficiency"))?,
            time_efficiency: u8::try_from(self.time_efficiency)
                .map_err(|_| invalid("time efficiency"))?,
            licensed_runs: self
                .licensed_runs
                .map(u64::try_from)
                .transpose()
                .map_err(|_| invalid("licensed runs"))?,
            connection_id: self.connection_id,
            character_name: self.character_name,
            location_id: self.location_id,
            location_name: self.location_name,
            location_flag: self.location_flag,
            observed_at: self.observed_at,
            used_by_builds: self
                .used_build_ids
                .unwrap_or_default()
                .into_iter()
                .zip(self.used_build_names.unwrap_or_default())
                .map(|(build_id, name)| BlueprintLibraryBuildRef {
                    build_id: BuildId(build_id),
                    name,
                })
                .collect(),
        })
    }
}

#[derive(sqlx::FromRow)]
struct SummaryRow {
    originals: i64,
    copies: i64,
    characters: i64,
    latest_observed_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct FacetRow {
    facet: String,
    value: String,
    label: String,
    count: i64,
    category_id: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use iskworks_core::BlueprintLibraryQuery;

    struct Seed {
        workspace_id: WorkspaceId,
        connection_id: Uuid,
        rifter_bpc: Uuid,
    }

    /// Valka owns, at Jita: a Rifter BPO (ME 10), a Rifter BPC with 5 runs
    /// (ME 8), an exhausted Merlin BPC, a Fullerite reaction formula, and a
    /// blueprint whose type is missing from the SDE. One Build uses the
    /// Rifter BPC. A second workspace owns a Rifter BPO that must never leak.
    async fn seed(pool: &PgPool) -> Seed {
        let workspace_id = WorkspaceId(Uuid::new_v4());
        let owner_id = Uuid::new_v4();
        let connection_id = Uuid::new_v4();
        let import_id = Uuid::new_v4();
        let rifter_bpc = Uuid::new_v4();
        let now = crate::db_now();
        let mut tx = pool.begin().await.expect("transaction begins");
        sqlx::query("INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','test','blueprint-library','active',true,$2,$2)")
            .bind(import_id).bind(now).execute(&mut *tx).await.expect("import");
        sqlx::query("INSERT INTO sde_categories (import_id,category_id,name_en,published) VALUES ($1,6,'Ship',true),($1,4,'Material',true),($1,9,'Blueprint',true),($1,24,'Reaction',true)")
            .bind(import_id).execute(&mut *tx).await.expect("categories");
        sqlx::query("INSERT INTO sde_groups (import_id,group_id,name_en,category_id,published) VALUES ($1,25,'Frigate',6,true),($1,429,'Composite',4,true),($1,105,'Frigate Blueprint',9,true),($1,1888,'Composite Reaction Formulas',24,true)")
            .bind(import_id).execute(&mut *tx).await.expect("groups");
        sqlx::query("INSERT INTO sde_types (import_id,type_id,name_en,group_id,group_name_en,published) VALUES ($1,587,'Rifter',25,'Frigate',true),($1,691,'Rifter Blueprint',105,'Frigate Blueprint',true),($1,603,'Merlin',25,'Frigate',true),($1,950,'Merlin Blueprint',105,'Frigate Blueprint',true),($1,16670,'Crystalline Carbonide',429,'Composite',true),($1,46166,'Crystalline Carbonide Reaction Formula',1888,'Composite Reaction Formulas',true)")
            .bind(import_id).execute(&mut *tx).await.expect("types");
        sqlx::query("INSERT INTO sde_blueprints (import_id,blueprint_type_id,name_en) VALUES ($1,691,'Rifter Blueprint'),($1,950,'Merlin Blueprint')")
            .bind(import_id).execute(&mut *tx).await.expect("blueprints");
        sqlx::query("INSERT INTO sde_blueprint_products (import_id,blueprint_type_id,product_type_id,quantity,position) VALUES ($1,691,587,1,0),($1,950,603,1,0)")
            .bind(import_id).execute(&mut *tx).await.expect("blueprint products");
        sqlx::query("INSERT INTO sde_reaction_formulas (import_id,reaction_formula_type_id,name_en) VALUES ($1,46166,'Crystalline Carbonide Reaction Formula')")
            .bind(import_id).execute(&mut *tx).await.expect("formula");
        sqlx::query("INSERT INTO sde_reaction_formula_products (import_id,reaction_formula_type_id,product_type_id,quantity,position) VALUES ($1,46166,16670,10000,0)")
            .bind(import_id).execute(&mut *tx).await.expect("formula products");

        let other_workspace = Uuid::new_v4();
        let other_owner = Uuid::new_v4();
        for (workspace, owner) in [(workspace_id.0, owner_id), (other_workspace, other_owner)] {
            sqlx::query("INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Blueprints',$2,$3,$3)")
                .bind(workspace).bind(owner).bind(now).execute(&mut *tx).await.expect("workspace");
            sqlx::query("INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Owner',true,$3,$3)")
                .bind(owner).bind(workspace).bind(now).execute(&mut *tx).await.expect("owner");
        }
        sqlx::query("INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,status,granted_scopes,connected_at,updated_at) VALUES ($1,$2,$3,9001,'Valka','connected','{}',$4,$4)")
            .bind(connection_id).bind(workspace_id.0).bind(owner_id).bind(now).execute(&mut *tx).await.expect("connection");
        sqlx::query("INSERT INTO market_location_names (workspace_id,location_id,location_name,owner_id,solar_system_id,resolved_by_connection_id,resolved_at,updated_at) VALUES ($1,60003760,'Jita IV - Moon 4',1000125,30000142,$2,$3,$3)")
            .bind(workspace_id.0).bind(connection_id).bind(now).execute(&mut *tx).await.expect("location");

        let observations: [(
            Uuid,
            Uuid,
            Uuid,
            i64,
            i64,
            &str,
            &str,
            i16,
            i16,
            Option<i64>,
        ); 6] = [
            (
                Uuid::new_v4(),
                workspace_id.0,
                owner_id,
                1,
                691,
                "Rifter Blueprint",
                "original",
                10,
                20,
                None,
            ),
            (
                rifter_bpc,
                workspace_id.0,
                owner_id,
                2,
                691,
                "Rifter Blueprint",
                "copy",
                8,
                16,
                Some(5),
            ),
            (
                Uuid::new_v4(),
                workspace_id.0,
                owner_id,
                3,
                950,
                "Merlin Blueprint",
                "copy",
                10,
                20,
                None,
            ),
            (
                Uuid::new_v4(),
                workspace_id.0,
                owner_id,
                4,
                46166,
                "Crystalline Carbonide Reaction Formula",
                "original",
                0,
                0,
                None,
            ),
            (
                Uuid::new_v4(),
                workspace_id.0,
                owner_id,
                5,
                999_999,
                "Mystery Blueprint",
                "original",
                0,
                0,
                None,
            ),
            (
                Uuid::new_v4(),
                other_workspace,
                other_owner,
                6,
                691,
                "Rifter Blueprint",
                "original",
                10,
                20,
                None,
            ),
        ];
        for (id, workspace, owner, item, type_id, name, kind, me, te, runs) in observations {
            let connection = (workspace == workspace_id.0).then_some(connection_id);
            sqlx::query(
                r#"INSERT INTO blueprint_observations (
                     id,workspace_id,owner_id,connection_id,eve_item_id,blueprint_type_id,
                     captured_blueprint_name,blueprint_kind,material_efficiency,time_efficiency,
                     licensed_runs,location_id,location_flag,observed_at,source_checksum
                   ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,60003760,'Hangar',$12,'checksum')"#,
            )
            .bind(id)
            .bind(workspace)
            .bind(owner)
            .bind(connection)
            .bind(item)
            .bind(type_id)
            .bind(name)
            .bind(kind)
            .bind(me)
            .bind(te)
            .bind(runs)
            .bind(now)
            .execute(&mut *tx)
            .await
            .expect("observation");
        }

        let build_id = Uuid::new_v4();
        sqlx::query("INSERT INTO builds (id,workspace_id,owner_id,display_name,blueprint_type_id,blueprint_name,product_type_id,product_name,product_quantity_per_run,source_sde_dataset_id,source_sde_version,recipe_fingerprint,runs,created_at,updated_at,recipe_kind,plan_root_build_id) VALUES ($1,$2,$3,'Rifter run',691,'Rifter Blueprint',587,'Rifter',1,$4,'test','fingerprint',1,$5,$5,'manufacturing',$1)")
            .bind(build_id).bind(workspace_id.0).bind(owner_id).bind(import_id).bind(now)
            .execute(&mut *tx).await.expect("build");
        sqlx::query("INSERT INTO build_draft_planning (build_id,planning_input,updated_at) VALUES ($1,$2,$3)")
            .bind(build_id)
            .bind(serde_json::json!({"blueprintSelection": {"mode": "observedAsset", "observationId": rifter_bpc.to_string()}}))
            .bind(now)
            .execute(&mut *tx).await.expect("draft planning");
        tx.commit().await.expect("fixtures committed");
        Seed {
            workspace_id,
            connection_id,
            rifter_bpc,
        }
    }

    async fn list(
        pool: &PgPool,
        workspace_id: WorkspaceId,
        query: BlueprintLibraryQuery,
    ) -> BlueprintLibraryPage {
        PgBlueprintLibraryRepository::new(pool.clone())
            .list(workspace_id, &query.validate().expect("valid query"))
            .await
            .expect("library loads")
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    async fn lists_workspace_blueprints_classified_by_product(pool: PgPool) {
        let seed = seed(&pool).await;

        let page = list(&pool, seed.workspace_id, BlueprintLibraryQuery::default()).await;

        assert_eq!(page.total, 5, "the other workspace's blueprint never leaks");
        let names = page
            .rows
            .iter()
            .map(|row| row.blueprint_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "Crystalline Carbonide Reaction Formula",
                "Merlin Blueprint",
                "Mystery Blueprint",
                "Rifter Blueprint",
                "Rifter Blueprint",
            ]
        );
        let formula = &page.rows[0];
        assert_eq!(formula.activity, BlueprintActivity::Reaction);
        assert_eq!(
            formula.product_name.as_deref(),
            Some("Crystalline Carbonide")
        );
        assert_eq!(formula.product_category_name.as_deref(), Some("Material"));
        let mystery = &page.rows[2];
        assert_eq!(mystery.activity, BlueprintActivity::Manufacturing);
        assert_eq!(mystery.product_type_id, None);
        let rifter_copy = page
            .rows
            .iter()
            .find(|row| row.observation_id == seed.rifter_bpc)
            .expect("rifter copy listed");
        assert_eq!(rifter_copy.product_group_name.as_deref(), Some("Frigate"));
        assert_eq!(rifter_copy.product_category_id, Some(6));
        assert_eq!(rifter_copy.licensed_runs, Some(5));
        assert_eq!(rifter_copy.character_name.as_deref(), Some("Valka"));
        assert_eq!(
            rifter_copy.location_name.as_deref(),
            Some("Jita IV - Moon 4")
        );
        assert_eq!(rifter_copy.used_by_builds.len(), 1);
        assert_eq!(rifter_copy.used_by_builds[0].name, "Rifter run");
        assert_eq!(
            page.summary,
            BlueprintLibrarySummary {
                originals: 3,
                copies: 2,
                characters: 1,
                latest_observed_at: page.summary.latest_observed_at,
            }
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    async fn filters_by_kind_category_efficiency_usability_and_activity(pool: PgPool) {
        let seed = seed(&pool).await;
        let names = |page: BlueprintLibraryPage| {
            page.rows
                .into_iter()
                .map(|row| (row.blueprint_name, row.kind))
                .collect::<Vec<_>>()
        };

        let copies = list(
            &pool,
            seed.workspace_id,
            BlueprintLibraryQuery {
                kinds: vec![BlueprintKind::Copy],
                ..Default::default()
            },
        )
        .await;
        assert_eq!(copies.total, 2);

        let ships_usable = list(
            &pool,
            seed.workspace_id,
            BlueprintLibraryQuery {
                category_ids: vec![6],
                usable_only: true,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            names(ships_usable),
            [
                ("Rifter Blueprint".to_string(), BlueprintKind::Original),
                ("Rifter Blueprint".to_string(), BlueprintKind::Copy),
            ],
            "the exhausted Merlin copy is not usable"
        );

        let me_ten = list(
            &pool,
            seed.workspace_id,
            BlueprintLibraryQuery {
                min_me: Some(10),
                group_ids: vec![25],
                ..Default::default()
            },
        )
        .await;
        assert_eq!(me_ten.total, 2, "Rifter BPO and Merlin BPC are ME 10");

        let reactions = list(
            &pool,
            seed.workspace_id,
            BlueprintLibraryQuery {
                activity: Some(BlueprintActivity::Reaction),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(reactions.total, 1);

        let searched = list(
            &pool,
            seed.workspace_id,
            BlueprintLibraryQuery {
                search: "merl".to_string(),
                connection_ids: vec![seed.connection_id],
                location_ids: vec![60003760],
                ..Default::default()
            },
        )
        .await;
        assert_eq!(searched.total, 1);

        let wildcard = list(
            &pool,
            seed.workspace_id,
            BlueprintLibraryQuery {
                search: "%".to_string(),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(wildcard.total, 0, "LIKE wildcards are matched literally");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    async fn sorts_runs_with_originals_as_unlimited_and_pages_with_a_stable_total(pool: PgPool) {
        let seed = seed(&pool).await;

        let by_runs = list(
            &pool,
            seed.workspace_id,
            BlueprintLibraryQuery {
                sort: BlueprintLibrarySort::Runs,
                order: BlueprintLibraryOrder::Desc,
                category_ids: vec![6],
                ..Default::default()
            },
        )
        .await;
        let runs = by_runs
            .rows
            .iter()
            .map(|row| (row.kind, row.licensed_runs))
            .collect::<Vec<_>>();
        assert_eq!(
            runs,
            [
                (BlueprintKind::Original, None),
                (BlueprintKind::Copy, Some(5)),
                (BlueprintKind::Copy, None),
            ]
        );

        let second = list(
            &pool,
            seed.workspace_id,
            BlueprintLibraryQuery {
                limit: Some(2),
                offset: Some(2),
                ..Default::default()
            },
        )
        .await;
        assert_eq!((second.rows.len(), second.total), (2, 5));
        let past_end = list(
            &pool,
            seed.workspace_id,
            BlueprintLibraryQuery {
                limit: Some(2),
                offset: Some(10),
                ..Default::default()
            },
        )
        .await;
        assert_eq!((past_end.rows.len(), past_end.total), (0, 5));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    async fn facets_cover_the_workspace_and_skip_unclassified_buckets(pool: PgPool) {
        let seed = seed(&pool).await;

        let facets = PgBlueprintLibraryRepository::new(pool.clone())
            .facets(seed.workspace_id)
            .await
            .expect("facets load");

        let categories = facets
            .categories
            .iter()
            .map(|option| (option.label.as_str(), option.count))
            .collect::<Vec<_>>();
        assert_eq!(categories, [("Material", 1), ("Ship", 3)]);
        let frigate = facets
            .groups
            .iter()
            .find(|group| group.label == "Frigate")
            .expect("frigate group");
        assert_eq!((frigate.count, frigate.category_id), (3, Some(6)));
        assert_eq!(facets.characters.len(), 1);
        assert_eq!(facets.characters[0].count, 5);
        assert_eq!(facets.locations[0].label, "Jita IV - Moon 4");
    }
}
