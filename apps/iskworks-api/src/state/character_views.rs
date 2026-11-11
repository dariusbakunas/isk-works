//! `AppState`'s character views dependencies: the [`CharacterViewDeps`] group, its
//! `with_*` builders and accessors.

use super::{esi_not_configured, AppState};
use crate::error::ApiError;
use iskworks_app::{
    CalendarService, CharacterRosterService, PlanetaryCharacterSource, PlanetaryService,
};
use std::sync::Arc;

/// Character read models built on synced ESI data: roster, calendar, jobs, planetary, skills, fittings and the asset browser.
#[derive(Clone, Default)]
pub(crate) struct CharacterViewDeps {
    pub(super) character_roster_service: Option<Arc<CharacterRosterService>>,
    pub(super) calendar_service: Option<Arc<CalendarService>>,
    pub(super) jobs_service: Option<Arc<iskworks_app::JobsService>>,
    pub(super) planetary_characters: Option<Arc<dyn PlanetaryCharacterSource>>,
    pub(super) skills_characters: Option<Arc<dyn iskworks_app::SkillsCharacterSource>>,
    pub(super) skill_plan_repository:
        Option<Arc<dyn iskworks_core::skill_plans::SkillPlanRepository>>,
    /// Fit stats; absent when the engine data is missing or stale, which
    /// leaves the rest of the API working.
    pub(crate) fitting_engine: Option<Arc<dyn iskworks_core::fitting::FittingEngine>>,
    pub(super) fit_repository: Option<Arc<dyn iskworks_core::fitting::FitRepository>>,
    pub(super) planetary_preferences:
        Option<Arc<dyn iskworks_core::planetary::PlanetaryPreferencesRepository>>,
    pub(super) pi_planner_repository:
        Option<Arc<dyn iskworks_core::pi_planner::PiPlannerInputsRepository>>,
    pub(super) asset_browser_repository: Option<Arc<iskworks_storage::PgAssetBrowserRepository>>,
    pub(super) fit_asset_holdings: Option<Arc<dyn iskworks_core::fitting::AssetHoldingsRepository>>,
}

impl AppState {
    /// Planetary Interaction data sources, normally wired by [`Self::with_esi`]
    /// from the Postgres ESI repository; tests substitute fakes.
    #[must_use]
    pub fn with_planetary_repositories(
        mut self,
        characters: Arc<dyn PlanetaryCharacterSource>,
        preferences: Arc<dyn iskworks_core::planetary::PlanetaryPreferencesRepository>,
    ) -> Self {
        self.character_views.planetary_characters = Some(characters);
        self.character_views.planetary_preferences = Some(preferences);
        self
    }

    #[must_use]
    pub fn with_pi_planner_repository(
        mut self,
        repository: Arc<dyn iskworks_core::pi_planner::PiPlannerInputsRepository>,
    ) -> Self {
        self.character_views.pi_planner_repository = Some(repository);
        self
    }

    /// The Skills page's character source, normally wired by
    /// [`Self::with_esi`] from the Postgres ESI repository; tests substitute
    /// fakes.
    #[must_use]
    pub fn with_skills_characters(
        mut self,
        characters: Arc<dyn iskworks_app::SkillsCharacterSource>,
    ) -> Self {
        self.character_views.skills_characters = Some(characters);
        self
    }

    #[must_use]
    pub fn with_fitting_engine(
        mut self,
        engine: Arc<dyn iskworks_core::fitting::FittingEngine>,
    ) -> Self {
        self.character_views.fitting_engine = Some(engine);
        self
    }

    #[must_use]
    pub fn with_fit_repository(
        mut self,
        repository: Arc<dyn iskworks_core::fitting::FitRepository>,
    ) -> Self {
        self.character_views.fit_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_skill_plan_repository(
        mut self,
        repository: Arc<dyn iskworks_core::skill_plans::SkillPlanRepository>,
    ) -> Self {
        self.character_views.skill_plan_repository = Some(repository);
        self
    }

    /// For tests: the Jobs read model with fake sources.
    #[must_use]
    pub fn with_jobs_service(mut self, service: Arc<iskworks_app::JobsService>) -> Self {
        self.character_views.jobs_service = Some(service);
        self
    }

    #[must_use]
    pub fn with_asset_browser_repository(
        mut self,
        repository: Arc<iskworks_storage::PgAssetBrowserRepository>,
    ) -> Self {
        self.character_views.asset_browser_repository = Some(repository);
        self
    }

    /// What the workspace's characters own, for a fit's asset coverage.
    #[must_use]
    pub fn with_fit_asset_holdings(
        mut self,
        repository: Arc<dyn iskworks_core::fitting::AssetHoldingsRepository>,
    ) -> Self {
        self.character_views.fit_asset_holdings = Some(repository);
        self
    }

    pub(crate) fn character_roster_service(&self) -> Result<Arc<CharacterRosterService>, ApiError> {
        self.character_views
            .character_roster_service
            .clone()
            .ok_or_else(esi_not_configured)
    }

    pub(crate) fn planetary_service(&self) -> Result<PlanetaryService, ApiError> {
        Ok(PlanetaryService::new(
            self.character_views
                .planetary_characters
                .clone()
                .ok_or_else(esi_not_configured)?,
            self.character_views
                .planetary_preferences
                .clone()
                .ok_or_else(esi_not_configured)?,
            self.sde_repository.clone(),
            self.market_repository()?,
        ))
    }

    /// The PI Planner. Character prefill needs ESI; everything else works
    /// without it.
    pub(crate) fn pi_planner_service(&self) -> Result<iskworks_app::PiPlannerService, ApiError> {
        let inputs = self.character_views.pi_planner_repository.clone().ok_or(
            ApiError::ServiceUnavailable("PI Planner persistence is unavailable."),
        )?;
        let service = iskworks_app::PiPlannerService::new(
            self.sde_repository.clone(),
            self.market_repository()?,
            inputs,
        );
        Ok(match self.character_views.planetary_characters.clone() {
            Some(characters) => service.with_characters(characters),
            None => service,
        })
    }

    pub(crate) fn skill_catalogue_service(
        &self,
    ) -> Result<iskworks_app::SkillCatalogueService, ApiError> {
        Ok(iskworks_app::SkillCatalogueService::new(
            self.character_views
                .skills_characters
                .clone()
                .ok_or_else(esi_not_configured)?,
            self.sde_repository.clone(),
        ))
    }

    pub(crate) fn type_info_service(&self) -> iskworks_app::TypeInfoService {
        let mut service = iskworks_app::TypeInfoService::new(self.sde_repository.clone());
        if let Some(characters) = self.character_views.skills_characters.clone() {
            service = service.with_characters(characters);
        }
        if let Some(market) = self.market.market_repository.clone() {
            service = service.with_market(market);
        }
        service
    }

    pub(crate) fn fitting_service(&self) -> Result<iskworks_app::FittingService, ApiError> {
        let mut service = iskworks_app::FittingService::new(
            self.character_views
                .fit_repository
                .clone()
                .ok_or(ApiError::ServiceUnavailable(
                    "Fit persistence is unavailable.",
                ))?,
            self.sde_repository.clone(),
        );
        if let Some(characters) = self.character_views.skills_characters.clone() {
            service = service.with_characters(characters);
        }
        if let Some(skill_plans) = self.character_views.skill_plan_repository.clone() {
            service = service.with_skill_plans(skill_plans);
        }
        if let Some(market) = self.market.market_repository.clone() {
            service = service.with_market(market);
        }
        if let Some(holdings) = self.character_views.fit_asset_holdings.clone() {
            service = service.with_asset_holdings(holdings);
        }
        if let Some(engine) = &self.character_views.fitting_engine {
            service = service.with_catalogue(engine.catalogue());
        }
        Ok(service)
    }

    /// The fitting engine, for anything that reads or writes fit text or
    /// calculates stats; absent when its data didn't load.
    pub(crate) fn fitting_engine(
        &self,
    ) -> Result<Arc<dyn iskworks_core::fitting::FittingEngine>, ApiError> {
        self.character_views
            .fitting_engine
            .clone()
            .ok_or(ApiError::FittingEngineUnavailable)
    }

    pub(crate) fn skill_plan_service(&self) -> Result<iskworks_app::SkillPlanService, ApiError> {
        Ok(iskworks_app::SkillPlanService::new(
            self.character_views.skill_plan_repository.clone().ok_or(
                ApiError::ServiceUnavailable("Skill plan persistence is unavailable."),
            )?,
            self.character_views
                .skills_characters
                .clone()
                .ok_or_else(esi_not_configured)?,
            self.sde_repository.clone(),
        ))
    }

    /// Calendar milestones, plus Planetary Interaction timers whenever the PI
    /// read model is available (it is optional: without it the calendar
    /// still serves industry and skill milestones).
    pub(crate) fn calendar_service(&self) -> Result<Arc<CalendarService>, ApiError> {
        let service =
            self.character_views
                .calendar_service
                .clone()
                .ok_or(ApiError::ServiceUnavailable(
                    "Calendar persistence is unavailable.",
                ))?;
        Ok(match self.planetary_service() {
            Ok(planetary) => Arc::new((*service).clone().with_planetary(Arc::new(planetary))),
            Err(_) => service,
        })
    }

    pub(crate) fn jobs_service(&self) -> Result<Arc<iskworks_app::JobsService>, ApiError> {
        self.character_views
            .jobs_service
            .clone()
            .ok_or(ApiError::ServiceUnavailable(
                "Jobs persistence is unavailable.",
            ))
    }

    pub(crate) fn asset_browser_repository(
        &self,
    ) -> Result<Arc<iskworks_storage::PgAssetBrowserRepository>, ApiError> {
        self.character_views
            .asset_browser_repository
            .clone()
            .ok_or(ApiError::ServiceUnavailable(
                "Asset browser persistence is unavailable.",
            ))
    }
}
