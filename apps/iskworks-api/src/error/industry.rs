use super::*;

pub(super) fn industry_response(error: IndustryError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        IndustryError::Validation(_)
        | IndustryError::InvalidMoney
        | IndustryError::InvalidRecipe
        | IndustryError::MoneyOverflow => (StatusCode::BAD_REQUEST, "validation_failed", false),
        IndustryError::BlueprintNotFound => (StatusCode::NOT_FOUND, "blueprint_not_found", false),
        IndustryError::ReactionFormulaNotFound => {
            (StatusCode::NOT_FOUND, "reaction_formula_not_found", false)
        }
        IndustryError::BuildNotFound => (StatusCode::NOT_FOUND, "build_not_found", false),
        IndustryError::ProducerInUse { .. } => (StatusCode::CONFLICT, "producer_in_use", false),
        IndustryError::CanonicalWrite(write) => {
            use iskworks_core::canonical_planner::CanonicalWriteError as E;
            match write {
                E::UnknownComponent { .. } => (StatusCode::BAD_REQUEST, "validation_failed", false),
                E::AmbiguousProducer { .. } => {
                    (StatusCode::CONFLICT, "canonical_producer_ambiguous", false)
                }
                E::WouldCreateCycle { .. } => {
                    (StatusCode::CONFLICT, "production_dependency_cycle", false)
                }
                E::RetiredConsumer { .. } | E::ProducerRecipeChange { .. } => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "canonical_producer_immutable",
                    false,
                ),
                E::PlanChanged => (StatusCode::CONFLICT, "canonical_plan_changed", true),
                E::CanonicalWriteRequired { .. } => {
                    (StatusCode::CONFLICT, "canonical_write_required", false)
                }
                E::CanonicalGraphCorrupt { .. } => {
                    (StatusCode::CONFLICT, "canonical_graph_invalid", false)
                }
            }
        }
        IndustryError::PriceSourceNotFound => {
            (StatusCode::NOT_FOUND, "price_source_not_found", false)
        }
        IndustryError::RevisionConflict => (StatusCode::CONFLICT, "revision_conflict", false),
        IndustryError::NoActiveSde => (
            StatusCode::SERVICE_UNAVAILABLE,
            "static_data_unavailable",
            true,
        ),
        IndustryError::StaticData(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "static_data_unavailable",
            true,
        ),
        IndustryError::Persistence(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_unavailable",
            true,
        ),
        IndustryError::Market(MarketError::OrdersUnavailable) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "market_orders_unavailable",
            false,
        ),
        IndustryError::Market(MarketError::InsufficientVolume) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "market_volume_insufficient",
            false,
        ),
        IndustryError::Market(MarketError::StaleObservations) => {
            (StatusCode::CONFLICT, "market_observations_stale", false)
        }
        IndustryError::Market(MarketError::Persistence(_)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_unavailable",
            true,
        ),
        IndustryError::Market(_) => (StatusCode::BAD_REQUEST, "market_pricing_failed", false),
        IndustryError::Facility(error) => {
            let (status, code) = facility_error_status(error);
            (status, code, false)
        }
        IndustryError::Blueprint(error) => match error {
            iskworks_core::BlueprintError::SelectionRequired => (
                StatusCode::BAD_REQUEST,
                "blueprint_selection_required",
                false,
            ),
            iskworks_core::BlueprintError::ManualKindUnknown => {
                (StatusCode::BAD_REQUEST, "blueprint_kind_unknown", false)
            }
            iskworks_core::BlueprintError::InvalidMaterialEfficiency => {
                (StatusCode::BAD_REQUEST, "blueprint_me_invalid", false)
            }
            iskworks_core::BlueprintError::InvalidTimeEfficiency => {
                (StatusCode::BAD_REQUEST, "blueprint_te_invalid", false)
            }
            iskworks_core::BlueprintError::CopyRunsRequired => (
                StatusCode::BAD_REQUEST,
                "blueprint_copy_runs_required",
                false,
            ),
            iskworks_core::BlueprintError::CopyRunsInsufficient { .. } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "blueprint_copy_runs_insufficient",
                false,
            ),
            iskworks_core::BlueprintError::ObservationNotFound => (
                StatusCode::NOT_FOUND,
                "blueprint_observation_not_found",
                false,
            ),
            iskworks_core::BlueprintError::ObservationOwnerMismatch => {
                (StatusCode::CONFLICT, "blueprint_owner_mismatch", false)
            }
            iskworks_core::BlueprintError::ObservationTypeMismatch => {
                (StatusCode::CONFLICT, "blueprint_type_mismatch", false)
            }
        },
    };
    let fields = match &error {
        IndustryError::ProducerInUse {
            producer,
            consumers,
        } => Some(std::collections::BTreeMap::from([
            ("producer".to_string(), producer.0.to_string()),
            (
                "consumers".to_string(),
                consumers
                    .iter()
                    .map(|consumer| consumer.0.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ])),
        _ => None,
    };
    (
        status,
        ErrorBody {
            code,
            message: error.to_string(),
            fields,
            retryable,
            correlation_id: None,
        },
    )
}
