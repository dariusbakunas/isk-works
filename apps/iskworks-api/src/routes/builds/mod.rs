//! Build routes: ordinary Build resource CRUD and library reads (`crud`),
//! and the build-plan preview pipeline with its market-coverage / EIV
//! orchestration (`preview`). Each child owns its handlers, DTOs, and a
//! `pub(super) fn router()`; this module composes them into the one
//! `builds::router()` that `build_router` merges.

use axum::Router;

use crate::AppState;

mod cost;
mod crud;
mod execution_plan;
mod export;
mod graph;
mod materials;
mod preview;
mod worksheet;

pub(crate) fn router() -> Router<AppState> {
    crud::router()
        .merge(preview::router())
        .merge(graph::router())
        .merge(materials::router())
        .merge(execution_plan::router())
        .merge(export::router())
        .merge(cost::router())
        .merge(worksheet::router())
}
