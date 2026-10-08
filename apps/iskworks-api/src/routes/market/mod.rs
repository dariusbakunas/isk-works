//! Market routes: reference-data / scope-selector reads, the item
//! browser, on-demand ESI refresh, manual price sources, CSV imports, and
//! player-structure market access. Each concern owns its handlers, DTOs,
//! and a `pub(super) fn router()`; this module composes them into the one
//! `market::router()` that `build_router` merges.

use axum::Router;

use crate::AppState;

mod catalog;
mod imports;
mod items;
mod price_sources;
mod refresh;
mod structures;

pub(crate) fn router() -> Router<AppState> {
    catalog::router()
        .merge(items::router())
        .merge(refresh::router())
        .merge(price_sources::router())
        .merge(imports::router())
        .merge(structures::router())
}
