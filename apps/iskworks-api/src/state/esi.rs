//! `AppState`'s esi dependencies: the [`EsiDeps`] group, its
//! `with_*` builders and accessors.

use super::{esi_not_configured, AppState};
use crate::error::ApiError;
use iskworks_app::{
    CalendarService, CharacterRosterService, CharacterSyncService, EsiApplicationError,
    EsiApplicationService, TicketJobService,
};
use std::sync::Arc;

/// EVE ESI: its persistence, the application service and character sync, and the downtime transport.
#[derive(Clone, Default)]
pub(crate) struct EsiDeps {
    pub(super) esi_repository: Option<Arc<iskworks_storage::PgEsiRepository>>,
    pub(crate) esi_service: Option<Arc<EsiApplicationService>>,
    pub(super) character_sync_service: Option<Arc<CharacterSyncService>>,
    /// Answers "is ESI down right now?" for the web app's badge.
    pub(super) esi_status: Option<iskworks_esi::HttpEsiTransport>,
}

impl AppState {
    /// The public ESI transport `/api/esi/status` asks about downtime.
    /// Unset, ESI is always reported available.
    #[must_use]
    pub fn with_esi_status(mut self, transport: iskworks_esi::HttpEsiTransport) -> Self {
        self.esi.esi_status = Some(transport);
        self
    }

    pub(crate) fn esi_status(&self) -> Option<&iskworks_esi::HttpEsiTransport> {
        self.esi.esi_status.as_ref()
    }

    #[must_use]
    pub fn with_esi(
        mut self,
        repository: Arc<iskworks_storage::PgEsiRepository>,
        service: Option<EsiApplicationService>,
    ) -> Self {
        self.market.adjusted_price_repository = Some(repository.clone());
        self.esi.esi_repository = Some(repository.clone());
        let character_roster_service = Arc::new(CharacterRosterService::new(
            repository.clone(),
            self.sde_repository.clone(),
        ));
        self.character_views.calendar_service = Some(Arc::new(
            CalendarService::new(character_roster_service.clone()).with_events(repository.clone()),
        ));
        self.character_views.jobs_service = Some(Arc::new(iskworks_app::JobsService::new(
            character_roster_service.clone(),
            repository.clone(),
        )));
        self.character_views.character_roster_service = Some(character_roster_service);
        self.character_views.planetary_characters = Some(repository.clone());
        self.character_views.skills_characters = Some(repository.clone());
        self.character_views.planetary_preferences = Some(repository.clone());
        let service = service.map(Arc::new);
        self.esi.character_sync_service = service.as_ref().map(|service| {
            Arc::new(CharacterSyncService::new(
                repository,
                service.transport(),
                Arc::clone(service),
                Arc::clone(service),
            ))
        });
        self.esi.esi_service = service;
        self
    }

    pub(crate) fn esi_repository(
        &self,
    ) -> Result<Arc<iskworks_storage::PgEsiRepository>, ApiError> {
        self.esi.esi_repository.clone().ok_or_else(|| {
            ApiError::Integration(EsiApplicationError::Configuration(
                "ESI persistence is unavailable.".to_string(),
            ))
        })
    }

    pub(crate) fn esi_service(&self) -> Result<Arc<EsiApplicationService>, ApiError> {
        self.esi.esi_service.clone().ok_or_else(esi_not_configured)
    }

    /// Character sync, linking Board tickets to industry jobs after each
    /// jobs refresh whenever ticket links are available.
    pub(crate) fn character_sync_service(&self) -> Result<Arc<CharacterSyncService>, ApiError> {
        let service = self
            .esi
            .character_sync_service
            .clone()
            .ok_or_else(esi_not_configured)?;
        Ok(match self.ticket_job_service() {
            Ok(ticket_jobs) => Arc::new(
                (*service)
                    .clone()
                    .with_industry_jobs_synced(Arc::new(ticket_jobs)),
            ),
            Err(_) => service,
        })
    }

    /// Ticket <-> ESI industry job links; needs ESI persistence.
    pub(crate) fn ticket_job_service(&self) -> Result<TicketJobService, ApiError> {
        let links =
            self.industry
                .ticket_job_repository
                .clone()
                .ok_or(ApiError::ServiceUnavailable(
                    "Ticket job persistence is unavailable.",
                ))?;
        Ok(TicketJobService::new(self.esi_repository()?, links))
    }
}
