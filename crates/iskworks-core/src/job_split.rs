//! How a run count splits into the industry jobs the player actually starts
//! in game. A blueprint copy licenses at most `L` runs per job, so a Build
//! of `R` runs on identical copies is `ceil(R / L)` jobs -- `R div L` full
//! jobs plus one remainder job. EVE rounds materials per job, so planning
//! math that sums per-job quantities needs this split rather than `R`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSplit {
    pub full_jobs: u64,
    pub runs_per_full_job: u64,
    pub remainder_runs: u64,
}

impl JobSplit {
    /// `max_runs_per_job` of `None` or `Some(0)` means "no per-job limit":
    /// one job of all `runs` (an Original, or a copy of unknown runs).
    #[must_use]
    pub fn for_runs(runs: u64, max_runs_per_job: Option<u64>) -> Self {
        match max_runs_per_job.filter(|max| *max > 0 && *max < runs) {
            Some(max) => Self {
                full_jobs: runs / max,
                runs_per_full_job: max,
                remainder_runs: runs % max,
            },
            None => Self {
                full_jobs: 1,
                runs_per_full_job: runs,
                remainder_runs: 0,
            },
        }
    }

    #[must_use]
    pub fn job_count(&self) -> u64 {
        self.full_jobs + u64::from(self.remainder_runs > 0)
    }

    /// Run count of the longest job -- the wall-clock duration basis when
    /// the jobs run in parallel slots.
    #[must_use]
    pub fn longest_job_runs(&self) -> u64 {
        self.runs_per_full_job
    }

    /// `(runs_per_job, job_count)` groups, at most two: the full jobs, then
    /// the remainder job. Empty groups are omitted.
    pub fn jobs(&self) -> impl Iterator<Item = (u64, u64)> {
        [
            (self.runs_per_full_job, self.full_jobs),
            (self.remainder_runs, 1),
        ]
        .into_iter()
        .filter(|(runs, count)| *runs > 0 && *count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups(split: JobSplit) -> Vec<(u64, u64)> {
        split.jobs().collect()
    }

    #[test]
    fn four_runs_on_one_run_copies_is_four_jobs() {
        let split = JobSplit::for_runs(4, Some(1));
        assert_eq!(split.job_count(), 4);
        assert_eq!(split.longest_job_runs(), 1);
        assert_eq!(groups(split), vec![(1, 4)]);
    }

    #[test]
    fn uneven_split_adds_one_remainder_job() {
        let split = JobSplit::for_runs(5, Some(2));
        assert_eq!(split.job_count(), 3);
        assert_eq!(split.longest_job_runs(), 2);
        assert_eq!(groups(split), vec![(2, 2), (1, 1)]);
    }

    #[test]
    fn no_limit_is_one_job_of_every_run() {
        for max in [None, Some(0), Some(3), Some(10)] {
            let split = JobSplit::for_runs(3, max);
            assert_eq!(split.job_count(), 1, "{max:?}");
            assert_eq!(split.longest_job_runs(), 3, "{max:?}");
            assert_eq!(groups(split), vec![(3, 1)], "{max:?}");
        }
    }
}
