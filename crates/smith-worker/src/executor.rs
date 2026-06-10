use temper_worker_protocol::{
    Assign, Branch, Failure, FailureClass, JobResult, ResultStatus, WORKER_PROTOCOL_VERSION,
};

#[derive(Clone, Debug, PartialEq)]
pub enum JobOutcome {
    Success {
        branch: Branch,
        summary: Option<String>,
    },
    Failure {
        class: FailureClass,
        message: String,
    },
}

pub trait JobExecutor {
    fn execute(&self, assign: Assign) -> impl std::future::Future<Output = JobOutcome> + Send;
}

#[derive(Clone, Debug, PartialEq)]
pub struct StubExecutor {
    mode: StubMode,
}

#[derive(Clone, Debug, PartialEq)]
enum StubMode {
    Success,
    Failure {
        class: FailureClass,
        message: String,
    },
}

impl StubExecutor {
    pub fn success() -> Self {
        Self {
            mode: StubMode::Success,
        }
    }

    pub fn failure(class: FailureClass, message: impl Into<String>) -> Self {
        Self {
            mode: StubMode::Failure {
                class,
                message: message.into(),
            },
        }
    }
}

impl JobExecutor for StubExecutor {
    fn execute(&self, assign: Assign) -> impl std::future::Future<Output = JobOutcome> + Send {
        let mode = self.mode.clone();
        async move {
            match mode {
                StubMode::Success => JobOutcome::Success {
                    branch: Branch {
                        name: format!("smith-worker/stub/{}", assign.job_id),
                        head_sha: "0000000000000000000000000000000000000000".to_string(),
                    },
                    summary: Some("stub executor completed without doing IO".to_string()),
                },
                StubMode::Failure { class, message } => JobOutcome::Failure { class, message },
            }
        }
    }
}

pub fn job_result(worker_id: &str, job_id: &str, outcome: JobOutcome) -> JobResult {
    match outcome {
        JobOutcome::Success { branch, summary } => JobResult {
            protocol_version: WORKER_PROTOCOL_VERSION,
            worker_id: worker_id.to_string(),
            job_id: job_id.to_string(),
            status: ResultStatus::Success,
            branch: Some(branch),
            failure: None,
            summary,
            details: None,
        },
        JobOutcome::Failure { class, message } => JobResult {
            protocol_version: WORKER_PROTOCOL_VERSION,
            worker_id: worker_id.to_string(),
            job_id: job_id.to_string(),
            status: ResultStatus::Failure,
            branch: None,
            failure: Some(Failure { class, message }),
            summary: None,
            details: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use temper_worker_protocol::Artifact;

    use super::*;

    fn assign(job_id: &str) -> Assign {
        Assign {
            protocol_version: WORKER_PROTOCOL_VERSION,
            job_id: job_id.to_string(),
            role: "coder".to_string(),
            repo: "ai/temper".to_string(),
            artifact: Artifact {
                item: json!(78),
                kind: "issue".to_string(),
            },
            job_payload: json!({}),
        }
    }

    #[tokio::test]
    async fn success_stub_maps_to_success_result_with_branch() {
        let outcome = StubExecutor::success().execute(assign("job-123")).await;
        let result = job_result("worker-1", "job-123", outcome);

        assert_eq!(result.protocol_version, WORKER_PROTOCOL_VERSION);
        assert_eq!(result.worker_id, "worker-1");
        assert_eq!(result.job_id, "job-123");
        assert_eq!(result.status, ResultStatus::Success);
        assert_eq!(result.failure, None);
        assert_eq!(
            result.summary.as_deref(),
            Some("stub executor completed without doing IO")
        );
        assert_eq!(
            result.branch,
            Some(Branch {
                name: "smith-worker/stub/job-123".to_string(),
                head_sha: "0000000000000000000000000000000000000000".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn failure_stub_maps_to_failure_result_without_branch() {
        let outcome = StubExecutor::failure(FailureClass::Permanent, "configured failure")
            .execute(assign("job-456"))
            .await;
        let result = job_result("worker-2", "job-456", outcome);

        assert_eq!(result.protocol_version, WORKER_PROTOCOL_VERSION);
        assert_eq!(result.worker_id, "worker-2");
        assert_eq!(result.job_id, "job-456");
        assert_eq!(result.status, ResultStatus::Failure);
        assert_eq!(result.branch, None);
        assert_eq!(result.summary, None);
        assert_eq!(
            result.failure,
            Some(Failure {
                class: FailureClass::Permanent,
                message: "configured failure".to_string(),
            })
        );
    }
}
