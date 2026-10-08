use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecipeCurrency {
    Current,
    OlderSdeVersion,
    RecipeChanged,
    BlueprintNoLongerAvailable,
    ReactionFormulaNoLongerAvailable,
    UnableToCompare,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedRecipeLine {
    pub type_id: i64,
    pub type_name: String,
    pub quantity_per_run: u64,
    pub sort_order: u32,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedRecipe {
    pub source_sde_dataset_id: Uuid,
    pub source_sde_version: String,
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub duration_seconds_per_run: Option<u64>,
    pub materials: Vec<CapturedRecipeLine>,
    pub products: Vec<CapturedRecipeLine>,
    pub fingerprint: String,
}

impl CapturedRecipe {
    pub fn capture(
        dataset_id: Uuid,
        source_version: String,
        recipe: ManufacturingRecipe,
    ) -> Result<Self, IndustryError> {
        let duration_seconds_per_run = recipe
            .duration_seconds
            .map(|value| u64::try_from(value).map_err(|_| IndustryError::InvalidRecipe))
            .transpose()?;
        let materials = capture_lines(recipe.materials)?;
        let products = capture_lines(recipe.products)?;
        if materials.is_empty() || products.is_empty() {
            return Err(IndustryError::InvalidRecipe);
        }
        let mut captured = Self {
            source_sde_dataset_id: dataset_id,
            source_sde_version: source_version,
            blueprint_type_id: recipe.blueprint_type_id,
            blueprint_name: recipe.blueprint_name,
            duration_seconds_per_run,
            materials,
            products,
            fingerprint: String::new(),
        };
        captured.fingerprint = captured.calculate_fingerprint();
        Ok(captured)
    }

    #[must_use]
    pub fn primary_product(&self) -> &CapturedRecipeLine {
        &self.products[0]
    }

    fn calculate_fingerprint(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(self.blueprint_type_id.to_be_bytes());
        digest.update(
            self.duration_seconds_per_run
                .unwrap_or_default()
                .to_be_bytes(),
        );
        for marker_and_lines in [("m", &self.materials), ("p", &self.products)] {
            digest.update(marker_and_lines.0.as_bytes());
            for line in marker_and_lines.1 {
                digest.update(line.type_id.to_be_bytes());
                digest.update(line.quantity_per_run.to_be_bytes());
                digest.update(line.sort_order.to_be_bytes());
            }
        }
        format!("{:x}", digest.finalize())
    }
}

fn capture_lines(
    lines: Vec<iskworks_sde::RecipeLine>,
) -> Result<Vec<CapturedRecipeLine>, IndustryError> {
    lines
        .into_iter()
        .enumerate()
        .map(|(position, line)| {
            Ok(CapturedRecipeLine {
                type_id: line.type_id,
                type_name: line.type_name,
                quantity_per_run: u64::try_from(line.quantity)
                    .map_err(|_| IndustryError::InvalidRecipe)?,
                sort_order: u32::try_from(position).map_err(|_| IndustryError::InvalidRecipe)?,
            })
        })
        .collect()
}

/// The reaction-formula counterpart of `CapturedRecipe`. Deliberately has no
/// material efficiency concept anywhere in its shape or its consumers:
/// reaction formulas have none in EVE (verified against real SDE dogma) --
/// the whole discount comes from facility
/// rigs, applied at preview time, not captured on the formula itself.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedReactionFormula {
    pub source_sde_dataset_id: Uuid,
    pub source_sde_version: String,
    pub reaction_formula_type_id: i64,
    pub reaction_formula_name: String,
    pub duration_seconds_per_run: Option<u64>,
    pub materials: Vec<CapturedRecipeLine>,
    pub products: Vec<CapturedRecipeLine>,
    pub fingerprint: String,
}

impl CapturedReactionFormula {
    pub fn capture(
        dataset_id: Uuid,
        source_version: String,
        recipe: iskworks_sde::ReactionFormulaRecipe,
    ) -> Result<Self, IndustryError> {
        let duration_seconds_per_run = recipe
            .duration_seconds
            .map(|value| u64::try_from(value).map_err(|_| IndustryError::InvalidRecipe))
            .transpose()?;
        let materials = capture_lines(recipe.materials)?;
        let products = capture_lines(recipe.products)?;
        if materials.is_empty() || products.is_empty() {
            return Err(IndustryError::InvalidRecipe);
        }
        let mut captured = Self {
            source_sde_dataset_id: dataset_id,
            source_sde_version: source_version,
            reaction_formula_type_id: recipe.reaction_formula_type_id,
            reaction_formula_name: recipe.reaction_formula_name,
            duration_seconds_per_run,
            materials,
            products,
            fingerprint: String::new(),
        };
        captured.fingerprint = captured.calculate_fingerprint();
        Ok(captured)
    }

    #[must_use]
    pub fn primary_product(&self) -> &CapturedRecipeLine {
        &self.products[0]
    }

    fn calculate_fingerprint(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(self.reaction_formula_type_id.to_be_bytes());
        digest.update(
            self.duration_seconds_per_run
                .unwrap_or_default()
                .to_be_bytes(),
        );
        for marker_and_lines in [("m", &self.materials), ("p", &self.products)] {
            digest.update(marker_and_lines.0.as_bytes());
            for line in marker_and_lines.1 {
                digest.update(line.type_id.to_be_bytes());
                digest.update(line.quantity_per_run.to_be_bytes());
                digest.update(line.sort_order.to_be_bytes());
            }
        }
        format!("{:x}", digest.finalize())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildRecipeKind {
    Manufacturing,
    Reaction,
}

/// A Build's recipe: either a manufacturing blueprint or a reaction formula.
/// Internally tagged so a manufacturing recipe keeps the exact JSON shape it
/// has today (plus an additive "kind" field) -- the SPA reads
/// `build.recipe.blueprintName`/`.blueprintTypeId` directly and must not
/// break.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BuildRecipe {
    Manufacturing(CapturedRecipe),
    Reaction(CapturedReactionFormula),
}

impl BuildRecipe {
    #[must_use]
    pub const fn kind(&self) -> BuildRecipeKind {
        match self {
            Self::Manufacturing(_) => BuildRecipeKind::Manufacturing,
            Self::Reaction(_) => BuildRecipeKind::Reaction,
        }
    }

    #[must_use]
    pub fn materials(&self) -> &[CapturedRecipeLine] {
        match self {
            Self::Manufacturing(recipe) => &recipe.materials,
            Self::Reaction(formula) => &formula.materials,
        }
    }

    #[must_use]
    pub fn products(&self) -> &[CapturedRecipeLine] {
        match self {
            Self::Manufacturing(recipe) => &recipe.products,
            Self::Reaction(formula) => &formula.products,
        }
    }

    #[must_use]
    pub fn primary_product(&self) -> &CapturedRecipeLine {
        match self {
            Self::Manufacturing(recipe) => recipe.primary_product(),
            Self::Reaction(formula) => formula.primary_product(),
        }
    }

    #[must_use]
    pub fn fingerprint(&self) -> &str {
        match self {
            Self::Manufacturing(recipe) => &recipe.fingerprint,
            Self::Reaction(formula) => &formula.fingerprint,
        }
    }

    #[must_use]
    pub const fn duration_seconds_per_run(&self) -> Option<u64> {
        match self {
            Self::Manufacturing(recipe) => recipe.duration_seconds_per_run,
            Self::Reaction(formula) => formula.duration_seconds_per_run,
        }
    }

    #[must_use]
    pub const fn source_sde_dataset_id(&self) -> Uuid {
        match self {
            Self::Manufacturing(recipe) => recipe.source_sde_dataset_id,
            Self::Reaction(formula) => formula.source_sde_dataset_id,
        }
    }

    #[must_use]
    pub fn source_sde_version(&self) -> &str {
        match self {
            Self::Manufacturing(recipe) => &recipe.source_sde_version,
            Self::Reaction(formula) => &formula.source_sde_version,
        }
    }

    /// Display name of the recipe itself (blueprint or formula name).
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Manufacturing(recipe) => &recipe.blueprint_name,
            Self::Reaction(formula) => &formula.reaction_formula_name,
        }
    }

    #[must_use]
    pub const fn blueprint_type_id(&self) -> Option<i64> {
        match self {
            Self::Manufacturing(recipe) => Some(recipe.blueprint_type_id),
            Self::Reaction(_) => None,
        }
    }

    #[must_use]
    pub const fn reaction_formula_type_id(&self) -> Option<i64> {
        match self {
            Self::Manufacturing(_) => None,
            Self::Reaction(formula) => Some(formula.reaction_formula_type_id),
        }
    }

    /// The blueprint name, for storage layers binding the nullable
    /// `blueprint_name` column -- `None` for a reaction recipe.
    #[must_use]
    pub fn name_if_manufacturing(&self) -> Option<&str> {
        match self {
            Self::Manufacturing(recipe) => Some(&recipe.blueprint_name),
            Self::Reaction(_) => None,
        }
    }

    /// The reaction formula name, for storage layers binding the nullable
    /// `reaction_formula_name` column -- `None` for a manufacturing recipe.
    #[must_use]
    pub fn name_if_reaction(&self) -> Option<&str> {
        match self {
            Self::Manufacturing(_) => None,
            Self::Reaction(formula) => Some(&formula.reaction_formula_name),
        }
    }
}

/// Which SDE recipe a Build command identifies -- a manufacturing blueprint
/// or a reaction formula. Nested under a named `recipe` field on the create
/// / update / preview commands, matching how `PlanBuildCommand.facility`
/// already nests its own tagged enum.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RecipeSelection {
    Manufacturing { blueprint_type_id: i64 },
    Reaction { reaction_formula_type_id: i64 },
}

pub fn recipe_selection_of(recipe: &BuildRecipe) -> RecipeSelection {
    match recipe {
        BuildRecipe::Manufacturing(recipe) => RecipeSelection::Manufacturing {
            blueprint_type_id: recipe.blueprint_type_id,
        },
        BuildRecipe::Reaction(formula) => RecipeSelection::Reaction {
            reaction_formula_type_id: formula.reaction_formula_type_id,
        },
    }
}

#[must_use]
pub fn build_market_coverage(recipe: &BuildRecipe) -> Vec<MarketCoverageRegistration> {
    recipe
        .materials()
        .iter()
        .chain(recipe.products())
        .fold(BTreeMap::new(), |mut items, line| {
            items
                .entry(line.type_id)
                .or_insert_with(|| line.type_name.clone());
            items
        })
        .into_iter()
        .map(|(type_id, type_name)| MarketCoverageRegistration { type_id, type_name })
        .collect()
}
