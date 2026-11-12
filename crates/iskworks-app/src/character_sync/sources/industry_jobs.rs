//! Industry jobs, recorded in the Jobs history.

use chrono::{DateTime, Utc};
use iskworks_core::{is_running_job_status, ConnectedCharacter, ConnectedCharacterId};
use iskworks_esi::CharacterIndustryJobObservation;
use serde_json::Value;

use crate::character_sync::{CharacterSyncService, SourceFailure};

impl CharacterSyncService {
    /// Records every returned job (running and the last 90 days of finished
    /// ones) in the Jobs history, and summarises only the running ones:
    /// roster slots, calendar and ticket linking read the summary and must
    /// see what they did before completed jobs were requested.
    pub(in crate::character_sync) async fn fetch_industry_jobs_summary(
        &self,
        access_token: &str,
        connection_id: ConnectedCharacterId,
        connection: &ConnectedCharacter,
        observed_at: DateTime<Utc>,
    ) -> Result<Value, SourceFailure> {
        let response = self
            .transport
            .character_industry_jobs(access_token, connection.eve_character_id)
            .await
            .map_err(SourceFailure::from)?;
        self.repository
            .record_industry_jobs(connection_id, &response.records, observed_at)
            .await
            .map_err(|error| SourceFailure::from(error.to_string()))?;
        let running: Vec<&CharacterIndustryJobObservation> = response
            .records
            .iter()
            .filter(|job| is_running_job_status(&job.status))
            .collect();
        let active_count = running.iter().filter(|job| job.status == "active").count();
        let jobs = serde_json::to_value(&running).map_err(SourceFailure::from)?;
        Ok(serde_json::json!({ "jobs": jobs, "activeCount": active_count }))
    }
}
