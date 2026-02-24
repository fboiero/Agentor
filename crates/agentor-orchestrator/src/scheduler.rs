use agentor_core::{AgentorError, AgentorResult};
use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// A single scheduled job definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub name: String,
    pub cron_expression: String,
    pub task_description: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Cron-based scheduler that manages a collection of [`ScheduledJob`]s.
///
/// The scheduler can compute next fire times, filter enabled jobs, and run a
/// background loop that logs when jobs should fire.
#[derive(Debug)]
pub struct Scheduler {
    jobs: Vec<ScheduledJob>,
}

impl Scheduler {
    /// Create a new scheduler with the given jobs.
    pub fn new(jobs: Vec<ScheduledJob>) -> Self {
        Self { jobs }
    }

    /// Parse a cron expression string into a [`cron::Schedule`].
    ///
    /// Uses the 7-field cron format: sec min hour day-of-month month day-of-week year.
    pub fn parse_cron(cron_expr: &str) -> AgentorResult<Schedule> {
        Schedule::from_str(cron_expr).map_err(|e| {
            AgentorError::Config(format!("Invalid cron expression '{cron_expr}': {e}"))
        })
    }

    /// Compute the next fire time for a given cron expression.
    ///
    /// Returns the first upcoming `DateTime<Utc>` after `Utc::now()`, or an error
    /// if the expression is invalid or has no upcoming times.
    pub fn next_fire_time(cron_expr: &str) -> AgentorResult<DateTime<Utc>> {
        let schedule = Self::parse_cron(cron_expr)?;
        schedule.upcoming(Utc).next().ok_or_else(|| {
            AgentorError::Config(format!(
                "Cron expression '{cron_expr}' has no upcoming fire times"
            ))
        })
    }

    /// Return references to only the enabled jobs.
    pub fn enabled_jobs(&self) -> Vec<&ScheduledJob> {
        self.jobs.iter().filter(|j| j.enabled).collect()
    }

    /// Return the total number of jobs (enabled and disabled).
    pub fn job_count(&self) -> usize {
        self.jobs.len()
    }

    /// Start the scheduler background loop.
    ///
    /// Spawns a tokio task that continuously:
    /// 1. Computes the next fire time for each enabled job.
    /// 2. Sleeps until the nearest fire time.
    /// 3. Logs every job whose fire time falls within a 1-second tolerance window.
    ///
    /// Returns the [`tokio::task::JoinHandle`] so the caller can abort or await it.
    ///
    /// Note: actual agent/task execution is intentionally not wired here to avoid
    /// circular dependencies with `agentor-agent`. The `tracing::info!` log acts as
    /// a hook point where execution can be integrated downstream.
    pub async fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let enabled: Vec<&ScheduledJob> =
                    self.jobs.iter().filter(|j| j.enabled).collect();

                if enabled.is_empty() {
                    tracing::info!("Scheduler: no enabled jobs, sleeping 60s");
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }

                // Compute next fire time for each enabled job.
                let mut nearest: Option<DateTime<Utc>> = None;
                let mut job_times: Vec<(&ScheduledJob, DateTime<Utc>)> = Vec::new();

                for job in &enabled {
                    match Self::next_fire_time(&job.cron_expression) {
                        Ok(next) => {
                            job_times.push((job, next));
                            nearest = Some(match nearest {
                                Some(cur) if next < cur => next,
                                Some(cur) => cur,
                                None => next,
                            });
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Scheduler: skipping job '{}' due to cron error: {}",
                                job.name,
                                e
                            );
                        }
                    }
                }

                let nearest = match nearest {
                    Some(t) => t,
                    None => {
                        tracing::warn!(
                            "Scheduler: all enabled jobs have invalid cron expressions, sleeping 60s"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                        continue;
                    }
                };

                // Sleep until the nearest fire time.
                let now = Utc::now();
                if nearest > now {
                    let wait = (nearest - now).to_std().unwrap_or_default();
                    tracing::info!("Scheduler: sleeping for {:?} until next job", wait);
                    tokio::time::sleep(wait).await;
                }

                // Fire all jobs within a 1-second tolerance window.
                let fire_threshold = Utc::now() + chrono::Duration::seconds(1);
                for (job, fire_time) in &job_times {
                    if *fire_time <= fire_threshold {
                        tracing::info!(
                            "Scheduler: firing job '{}' — {}",
                            job.name,
                            job.task_description
                        );
                    }
                }
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_cron() {
        let result = Scheduler::parse_cron("0 * * * * * *"); // every minute
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_invalid_cron() {
        let result = Scheduler::parse_cron("not a cron expression");
        assert!(result.is_err());
    }

    #[test]
    fn test_next_fire_time_is_future() {
        let next = Scheduler::next_fire_time("0 * * * * * *").unwrap();
        assert!(next > Utc::now());
    }

    #[test]
    fn test_enabled_jobs_filter() {
        let jobs = vec![
            ScheduledJob {
                name: "active".into(),
                cron_expression: "0 * * * * * *".into(),
                task_description: "do stuff".into(),
                enabled: true,
            },
            ScheduledJob {
                name: "inactive".into(),
                cron_expression: "0 * * * * * *".into(),
                task_description: "do other stuff".into(),
                enabled: false,
            },
        ];
        let scheduler = Scheduler::new(jobs);
        assert_eq!(scheduler.enabled_jobs().len(), 1);
        assert_eq!(scheduler.enabled_jobs()[0].name, "active");
    }

    #[test]
    fn test_job_count() {
        let scheduler = Scheduler::new(vec![ScheduledJob {
            name: "j1".into(),
            cron_expression: "0 * * * * * *".into(),
            task_description: "task".into(),
            enabled: true,
        }]);
        assert_eq!(scheduler.job_count(), 1);
    }
}
