use crate::report::{ComplianceFramework, ComplianceReport};
use agentor_core::{AgentorError, AgentorResult};
use chrono::Utc;
use std::path::PathBuf;

/// JSON-based persistence for compliance reports.
pub struct JsonReportStore {
    base_dir: PathBuf,
}

impl JsonReportStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Save a compliance report as a JSON file.
    /// Returns the path where the report was written.
    pub async fn save_report(&self, report: &ComplianceReport) -> AgentorResult<PathBuf> {
        tokio::fs::create_dir_all(&self.base_dir)
            .await
            .map_err(AgentorError::Io)?;

        let framework = framework_slug(&report.framework);
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("{}_{}.json", framework, timestamp);
        let path = self.base_dir.join(filename);

        let json = serde_json::to_string_pretty(report)?;
        tokio::fs::write(&path, json)
            .await
            .map_err(AgentorError::Io)?;

        Ok(path)
    }

    /// Load the most recent report for a given framework.
    pub async fn load_latest(
        &self,
        framework: ComplianceFramework,
    ) -> AgentorResult<Option<ComplianceReport>> {
        let slug = framework_slug(&framework);
        let reports = self.list_reports_for(slug).await?;

        match reports.last() {
            Some(path) => {
                let content = tokio::fs::read_to_string(path)
                    .await
                    .map_err(AgentorError::Io)?;
                let report: ComplianceReport = serde_json::from_str(&content)?;
                Ok(Some(report))
            }
            None => Ok(None),
        }
    }

    /// List all report files, sorted by name (ascending = oldest first).
    pub async fn list_reports(&self) -> AgentorResult<Vec<PathBuf>> {
        self.list_reports_for("").await
    }

    /// List report files for a specific framework prefix.
    async fn list_reports_for(&self, prefix: &str) -> AgentorResult<Vec<PathBuf>> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&self.base_dir)
            .await
            .map_err(AgentorError::Io)?;

        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".json") && (prefix.is_empty() || name.starts_with(prefix)) {
                    entries.push(path);
                }
            }
        }

        entries.sort();
        Ok(entries)
    }
}

fn framework_slug(framework: &ComplianceFramework) -> &'static str {
    match framework {
        ComplianceFramework::GDPR => "gdpr",
        ComplianceFramework::ISO27001 => "iso27001",
        ComplianceFramework::ISO42001 => "iso42001",
        ComplianceFramework::DPGA => "dpga",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{ComplianceStatus, Finding, Severity};

    fn sample_report(framework: ComplianceFramework) -> ComplianceReport {
        ComplianceReport {
            framework,
            status: ComplianceStatus::Compliant,
            findings: vec![Finding {
                id: "F-1".to_string(),
                framework,
                severity: Severity::Info,
                title: "Test finding".to_string(),
                description: "All good".to_string(),
                recommendation: String::new(),
                compliant: true,
            }],
            generated_at: Utc::now(),
            summary: "Test report".to_string(),
        }
    }

    #[tokio::test]
    async fn test_save_and_load_report() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonReportStore::new(dir.path());

        let report = sample_report(ComplianceFramework::ISO27001);
        let path = store.save_report(&report).await.unwrap();
        assert!(path.exists());
        assert!(path.to_str().unwrap().contains("iso27001_"));

        let loaded = store
            .load_latest(ComplianceFramework::ISO27001)
            .await
            .unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.summary, "Test report");
    }

    #[tokio::test]
    async fn test_load_latest_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonReportStore::new(dir.path());

        let result = store.load_latest(ComplianceFramework::GDPR).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_reports() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonReportStore::new(dir.path());

        store
            .save_report(&sample_report(ComplianceFramework::ISO27001))
            .await
            .unwrap();
        store
            .save_report(&sample_report(ComplianceFramework::GDPR))
            .await
            .unwrap();

        let all = store.list_reports().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_list_empty_dir() {
        let store = JsonReportStore::new("/tmp/agentor_nonexistent_compliance_test_dir");
        let reports = store.list_reports().await.unwrap();
        assert!(reports.is_empty());
    }
}
