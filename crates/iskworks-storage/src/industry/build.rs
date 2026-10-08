use super::*;

pub(super) fn recipe_kind_str(value: BuildRecipeKind) -> &'static str {
    match value {
        BuildRecipeKind::Manufacturing => "manufacturing",
        BuildRecipeKind::Reaction => "reaction",
    }
}

pub(super) async fn load_recipe_lines(
    pool: &PgPool,
    build_id: BuildId,
    table: &str,
) -> Result<Vec<CapturedRecipeLine>, IndustryError> {
    let sql = format!(
        "SELECT type_id, captured_name, quantity_per_run, sort_order FROM {table} WHERE build_id = $1 ORDER BY sort_order"
    );
    sqlx::query_as::<_, RecipeLineRow>(&sql)
        .bind(build_id.0)
        .fetch_all(pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(RecipeLineRow::into_line)
        .collect()
}

pub(super) async fn insert_recipe_lines(
    tx: &mut Transaction<'_, Postgres>,
    build_id: BuildId,
    table: &str,
    lines: &[CapturedRecipeLine],
) -> Result<(), IndustryError> {
    let sql = format!(
        "INSERT INTO {table} (build_id, type_id, captured_name, quantity_per_run, sort_order) VALUES ($1, $2, $3, $4, $5)"
    );
    for line in lines {
        sqlx::query(&sql)
            .bind(build_id.0)
            .bind(line.type_id)
            .bind(&line.type_name)
            .bind(i64_from_u64(line.quantity_per_run)?)
            .bind(i32::try_from(line.sort_order).map_err(|_| IndustryError::InvalidRecipe)?)
            .execute(&mut **tx)
            .await
            .map_err(map_error)?;
    }
    Ok(())
}

pub(super) async fn classify_build_update(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: WorkspaceId,
    build_id: BuildId,
) -> Result<IndustryError, IndustryError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM builds WHERE workspace_id = $1 AND id = $2)",
    )
    .bind(workspace_id.0)
    .bind(build_id.0)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_error)?;
    Ok(if exists {
        IndustryError::RevisionConflict
    } else {
        IndustryError::BuildNotFound
    })
}

pub(super) async fn load_draft_planning(
    pool: &PgPool,
    build_id: BuildId,
) -> Result<Option<DraftPlanningSnapshot>, IndustryError> {
    let row = sqlx::query_as::<_, (serde_json::Value, DateTime<Utc>)>(
        "SELECT planning_input, updated_at FROM build_draft_planning WHERE build_id = $1",
    )
    .bind(build_id.0)
    .fetch_optional(pool)
    .await
    .map_err(map_error)?;

    row.map(|(planning_input, updated_at)| {
        serde_json::from_value::<DraftPlanningInput>(planning_input)
            .map(|input| DraftPlanningSnapshot { input, updated_at })
            .map_err(|error| {
                IndustryError::Persistence(format!("invalid stored draft planning input: {error}"))
            })
    })
    .transpose()
}

pub(super) async fn write_draft_planning(
    tx: &mut Transaction<'_, Postgres>,
    build_id: BuildId,
    snapshot: Option<&DraftPlanningSnapshot>,
) -> Result<(), IndustryError> {
    if let Some(snapshot) = snapshot {
        let input = serde_json::to_value(&snapshot.input).map_err(|error| {
            IndustryError::Persistence(format!("could not serialize draft planning input: {error}"))
        })?;
        sqlx::query(
            r#"
            INSERT INTO build_draft_planning (build_id, planning_input, updated_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (build_id) DO UPDATE
            SET planning_input = EXCLUDED.planning_input,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(build_id.0)
        .bind(input)
        .bind(snapshot.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(map_error)?;
    } else {
        sqlx::query("DELETE FROM build_draft_planning WHERE build_id = $1")
            .bind(build_id.0)
            .execute(&mut **tx)
            .await
            .map_err(map_error)?;
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
pub(super) struct BuildRow {
    pub(super) id: Uuid,
    pub(super) workspace_id: Uuid,
    pub(super) owner_id: Uuid,
    pub(super) display_name: String,
    pub(super) recipe_kind: String,
    pub(super) blueprint_type_id: Option<i64>,
    pub(super) blueprint_name: Option<String>,
    pub(super) reaction_formula_type_id: Option<i64>,
    pub(super) reaction_formula_name: Option<String>,
    pub(super) duration_seconds_per_run: Option<i64>,
    pub(super) source_sde_dataset_id: Uuid,
    pub(super) source_sde_version: String,
    pub(super) recipe_fingerprint: String,
    pub(super) runs: i64,
    pub(super) notes: String,
    pub(super) revision: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

impl BuildRow {
    pub(super) fn into_build(
        self,
        materials: Vec<CapturedRecipeLine>,
        products: Vec<CapturedRecipeLine>,
        draft_planning: Option<DraftPlanningSnapshot>,
        recipe_currency: RecipeCurrency,
        active_sde_version: Option<String>,
    ) -> Result<Build, IndustryError> {
        let duration_seconds_per_run = self
            .duration_seconds_per_run
            .map(u64_from_i64)
            .transpose()?;
        let recipe = match self.recipe_kind.as_str() {
            "manufacturing" => {
                let blueprint_type_id = self.blueprint_type_id.ok_or_else(|| {
                    IndustryError::Persistence(
                        "manufacturing build is missing a blueprint identity".to_string(),
                    )
                })?;
                let blueprint_name = self.blueprint_name.ok_or_else(|| {
                    IndustryError::Persistence(
                        "manufacturing build is missing a blueprint identity".to_string(),
                    )
                })?;
                BuildRecipe::Manufacturing(CapturedRecipe {
                    source_sde_dataset_id: self.source_sde_dataset_id,
                    source_sde_version: self.source_sde_version,
                    blueprint_type_id,
                    blueprint_name,
                    duration_seconds_per_run,
                    materials,
                    products,
                    fingerprint: self.recipe_fingerprint,
                })
            }
            "reaction" => {
                let reaction_formula_type_id = self.reaction_formula_type_id.ok_or_else(|| {
                    IndustryError::Persistence(
                        "reaction build is missing a reaction formula identity".to_string(),
                    )
                })?;
                let reaction_formula_name = self.reaction_formula_name.ok_or_else(|| {
                    IndustryError::Persistence(
                        "reaction build is missing a reaction formula identity".to_string(),
                    )
                })?;
                BuildRecipe::Reaction(CapturedReactionFormula {
                    source_sde_dataset_id: self.source_sde_dataset_id,
                    source_sde_version: self.source_sde_version,
                    reaction_formula_type_id,
                    reaction_formula_name,
                    duration_seconds_per_run,
                    materials,
                    products,
                    fingerprint: self.recipe_fingerprint,
                })
            }
            other => {
                return Err(IndustryError::Persistence(format!(
                    "unknown build recipe kind {other}"
                )))
            }
        };
        Ok(Build {
            id: BuildId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            owner_id: iskworks_core::OwnerId(self.owner_id),
            name: self.display_name,
            recipe,
            runs: u64_from_i64(self.runs)?,
            notes: self.notes,
            revision: u64_from_i64(self.revision)?,
            created_at: self.created_at,
            updated_at: self.updated_at,
            draft_planning,
            recipe_currency,
            active_sde_version,
            // Read-model extras are resolved by `PgIndustryRepository::load_build`
            // after `into_build` returns (they need extra SDE / observation
            // queries this row-mapper deliberately doesn't do).
            product_category_name: None,
            product_group_name: None,
            selected_blueprint_origin: None,
            has_owned_blueprint: false,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct RecipeLineRow {
    pub(super) type_id: i64,
    pub(super) captured_name: String,
    pub(super) quantity_per_run: i64,
    pub(super) sort_order: i32,
}

impl RecipeLineRow {
    pub(super) fn into_line(self) -> Result<CapturedRecipeLine, IndustryError> {
        Ok(CapturedRecipeLine {
            type_id: self.type_id,
            type_name: self.captured_name,
            quantity_per_run: u64_from_i64(self.quantity_per_run)?,
            sort_order: u32::try_from(self.sort_order)
                .map_err(|_| IndustryError::Persistence("invalid sort order".to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct BlueprintObservationRow {
    pub(super) id: Uuid,
    pub(super) workspace_id: Uuid,
    pub(super) owner_id: Uuid,
    pub(super) eve_item_id: i64,
    pub(super) blueprint_type_id: i64,
    pub(super) captured_blueprint_name: String,
    pub(super) blueprint_kind: String,
    pub(super) material_efficiency: i16,
    pub(super) time_efficiency: i16,
    pub(super) licensed_runs: Option<i64>,
    pub(super) location_id: i64,
    pub(super) location_flag: String,
    pub(super) captured_location_name: Option<String>,
    pub(super) observed_at: DateTime<Utc>,
    pub(super) imported_at: DateTime<Utc>,
    pub(super) owner_name: String,
}

impl BlueprintObservationRow {
    pub(super) fn into_domain(self) -> Result<BlueprintObservation, IndustryError> {
        Ok(BlueprintObservation {
            id: self.id,
            workspace_id: WorkspaceId(self.workspace_id),
            owner_id: OwnerId(self.owner_id),
            owner_name: self.owner_name,
            eve_item_id: self.eve_item_id,
            blueprint_type_id: self.blueprint_type_id,
            blueprint_name: self.captured_blueprint_name,
            kind: parse_blueprint_kind(&self.blueprint_kind)?,
            material_efficiency: u8::try_from(self.material_efficiency)
                .map_err(|_| IndustryError::InvalidRecipe)?,
            time_efficiency: u8::try_from(self.time_efficiency)
                .map_err(|_| IndustryError::InvalidRecipe)?,
            licensed_runs: self.licensed_runs.map(u64_from_i64).transpose()?,
            location_id: self.location_id,
            location_flag: self.location_flag,
            location_name: self.captured_location_name,
            observed_at: self.observed_at,
            imported_at: self.imported_at,
        })
    }
}

pub(super) fn parse_blueprint_kind(value: &str) -> Result<BlueprintKind, IndustryError> {
    match value {
        "original" => Ok(BlueprintKind::Original),
        "copy" => Ok(BlueprintKind::Copy),
        "unknown" => Ok(BlueprintKind::Unknown),
        _ => Err(IndustryError::Persistence("invalid blueprint kind".into())),
    }
}

pub(super) async fn load_blueprint_observation(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    id: Uuid,
) -> Result<BlueprintObservation, IndustryError> {
    sqlx::query_as::<_, BlueprintObservationRow>("SELECT bo.id,bo.workspace_id,bo.owner_id,bo.eve_item_id,bo.blueprint_type_id,COALESCE(t.name_en,bo.captured_blueprint_name) AS captured_blueprint_name,bo.blueprint_kind,bo.material_efficiency,bo.time_efficiency,bo.licensed_runs,bo.location_id,bo.location_flag,COALESCE(bo.captured_location_name,mln.location_name) AS captured_location_name,bo.observed_at,bo.imported_at,o.display_name AS owner_name FROM blueprint_observations bo JOIN owners o ON o.id=bo.owner_id LEFT JOIN market_location_names mln ON mln.workspace_id=bo.workspace_id AND mln.location_id=bo.location_id LEFT JOIN sde_imports si ON si.active LEFT JOIN sde_types t ON t.import_id=si.id AND t.type_id=bo.blueprint_type_id WHERE bo.workspace_id=$1 AND bo.id=$2")
        .bind(workspace_id.0).bind(id).fetch_optional(pool).await.map_err(map_error)?
        .ok_or(IndustryError::Blueprint(iskworks_core::BlueprintError::ObservationNotFound))?.into_domain()
}

#[derive(sqlx::FromRow)]
pub(super) struct PlanRecipeLineRow {
    pub(super) build_id: Uuid,
    pub(super) type_id: i64,
    pub(super) captured_name: String,
    pub(super) quantity_per_run: i64,
    pub(super) sort_order: i32,
}

impl PlanRecipeLineRow {
    pub(super) fn into_line(self) -> Result<(Uuid, CapturedRecipeLine), IndustryError> {
        let build_id = self.build_id;
        RecipeLineRow {
            type_id: self.type_id,
            captured_name: self.captured_name,
            quantity_per_run: self.quantity_per_run,
            sort_order: self.sort_order,
        }
        .into_line()
        .map(|line| (build_id, line))
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct ProductionDependencyRow {
    pub(super) id: Uuid,
    pub(super) plan_root_build_id: Uuid,
    pub(super) consumer_build_id: Uuid,
    pub(super) component_type_id: i64,
    pub(super) sourcing: String,
    pub(super) method_kind: Option<String>,
    pub(super) method_type_id: Option<i64>,
    pub(super) producer_build_id: Option<Uuid>,
    pub(super) fulfillment_scope: String,
    pub(super) revision: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

impl ProductionDependencyRow {
    pub(super) fn into_domain(
        self,
    ) -> Result<iskworks_core::production_dependency::PersistedProductionDependency, IndustryError>
    {
        use iskworks_core::production_dependency::{DependencySourcing, ProductionMethod};
        let invalid = |what: &str| {
            IndustryError::Persistence(format!(
                "invalid production dependency {} ({what})",
                self.id
            ))
        };
        let sourcing = match (
            self.sourcing.as_str(),
            self.method_kind.as_deref(),
            self.method_type_id,
        ) {
            ("buy", None, None) => DependencySourcing::Buy,
            ("produce", Some("manufacturing"), Some(blueprint_type_id)) => {
                DependencySourcing::Produce {
                    method: ProductionMethod::Manufacturing { blueprint_type_id },
                }
            }
            ("produce", Some("reaction"), Some(reaction_formula_type_id)) => {
                DependencySourcing::Produce {
                    method: ProductionMethod::Reaction {
                        reaction_formula_type_id,
                    },
                }
            }
            _ => return Err(invalid("sourcing")),
        };
        let fulfillment_scope = match self.fulfillment_scope.as_str() {
            "missing" => iskworks_core::FulfillmentScope::Missing,
            "full" => iskworks_core::FulfillmentScope::Full,
            _ => return Err(invalid("fulfillment scope")),
        };
        Ok(
            iskworks_core::production_dependency::PersistedProductionDependency {
                id: self.id,
                plan_root_build_id: BuildId(self.plan_root_build_id),
                consumer_build_id: BuildId(self.consumer_build_id),
                component_type_id: self.component_type_id,
                sourcing,
                producer_build_id: self.producer_build_id.map(BuildId),
                fulfillment_scope,
                revision: u64_from_i64(self.revision)?,
                created_at: self.created_at,
                updated_at: self.updated_at,
            },
        )
    }
}

pub(super) fn method_columns(
    method: iskworks_core::production_dependency::ProductionMethod,
) -> (&'static str, i64) {
    use iskworks_core::production_dependency::ProductionMethod;
    match method {
        ProductionMethod::Manufacturing { blueprint_type_id } => {
            ("manufacturing", blueprint_type_id)
        }
        ProductionMethod::Reaction {
            reaction_formula_type_id,
        } => ("reaction", reaction_formula_type_id),
    }
}
