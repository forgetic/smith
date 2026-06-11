use std::collections::BTreeSet;
use std::sync::Arc;

use temper_worker_protocol::{
    Assign, ErrorCode, FailureClass, JobResult, ResultStatus, WorkerProtocolMessage,
};
use tokio::sync::mpsc;

use crate::client::{WorkerClient, WorkerError, heartbeat_message, poll_message, register_message};
use crate::config::WorkerConfig;
use crate::executor::{JobExecutor, job_result};

pub async fn run_worker<E>(config: WorkerConfig, executor: Arc<E>) -> Result<(), WorkerError>
where
    E: JobExecutor + Send + Sync + 'static,
{
    let client = WorkerClient::new(&config.daemon_url)?;
    client.send(&register_message(&config)).await?;
    let line = registered_worker_line(&config.worker_id, config.capabilities.len());
    eprintln!("{line}");

    let mut free_capacity = config.max_concurrent_jobs;
    let mut in_flight = BTreeSet::new();
    let (completed_tx, mut completed_rx) =
        mpsc::channel::<JobResult>(config.max_concurrent_jobs as usize);
    let mut heartbeat = tokio::time::interval(config.heartbeat_interval);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Tokio intervals complete immediately on their first tick. Consume that
    // tick outside the select loop so an idle worker starts by polling rather
    // than by taking an empty heartbeat branch first.
    heartbeat.tick().await;

    loop {
        let poll_free_capacity = free_capacity;
        tokio::select! {
            maybe_result = completed_rx.recv() => {
                let result = maybe_result.ok_or(WorkerError::ResultChannelClosed)?;
                in_flight.remove(&result.job_id);
                free_capacity = free_capacity.saturating_add(1).min(config.max_concurrent_jobs);

                match client.send(&WorkerProtocolMessage::Result(result.clone())).await? {
                    Some(WorkerProtocolMessage::Release(_)) | None => {}
                    Some(other) => eprintln!("smith-worker: unexpected result reply from daemon: {other:?}"),
                }
                let line = result_sent_line(&result);
                eprintln!("{line}");
            }
            _ = heartbeat.tick() => {
                if !in_flight.is_empty() {
                    if let Err(error) = client
                        .send(&heartbeat_message(&config, &in_flight, free_capacity))
                        .await
                    {
                        eprintln!("smith-worker: heartbeat failed: {error}");
                    }
                }
            }
            reply = async {
                let message = poll_message(&config, poll_free_capacity);
                client.send(&message).await
            }, if poll_free_capacity > 0 => {
                match reply? {
                    Some(WorkerProtocolMessage::Assign(assign)) => {
                        let line = assigned_job_line(&assign);
                        eprintln!("{line}");
                        let job_id = assign.job_id.clone();
                        free_capacity = free_capacity.saturating_sub(1);
                        in_flight.insert(job_id.clone());

                        let tx = completed_tx.clone();
                        let executor = Arc::clone(&executor);
                        let worker_id = config.worker_id.clone();
                        tokio::spawn(async move {
                            let outcome = executor.execute(assign).await;
                            let result = job_result(&worker_id, &job_id, outcome);
                            if tx.send(result).await.is_err() {
                                eprintln!("smith-worker: completed job result receiver closed");
                            }
                        });
                    }
                    Some(WorkerProtocolMessage::Error(error)) if error.code == ErrorCode::PollTimeout => {}
                    Some(other) => eprintln!("smith-worker: unexpected poll reply from daemon: {other:?}"),
                    None => eprintln!("smith-worker: empty poll reply from daemon"),
                }
            }
        }
    }
}

pub fn registered_worker_line(worker_id: &str, capability_count: usize) -> String {
    format!("smith-worker: registered worker_id={worker_id} capabilities={capability_count}")
}

pub fn assigned_job_line(assign: &Assign) -> String {
    format!(
        "smith-worker: assigned job_id={} role={} repo={}",
        assign.job_id, assign.role, assign.repo
    )
}

pub fn result_sent_line(result: &JobResult) -> String {
    format!(
        "smith-worker: result sent job_id={} status={}",
        result.job_id,
        result_status_display(result)
    )
}

fn result_status_display(result: &JobResult) -> String {
    match result.status {
        ResultStatus::Success => "success".to_string(),
        ResultStatus::Failure => {
            let class = result
                .failure
                .as_ref()
                .map(|failure| match failure.class {
                    FailureClass::Transient => "transient",
                    FailureClass::Permanent => "permanent",
                    FailureClass::Canceled => "canceled",
                    FailureClass::Protocol => "protocol",
                })
                .unwrap_or("unknown");
            format!("failure({class})")
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use temper_worker_protocol::{Artifact, Branch, Failure, WORKER_PROTOCOL_VERSION};

    use super::*;

    fn assign() -> Assign {
        Assign {
            protocol_version: WORKER_PROTOCOL_VERSION,
            job_id: "job-123".to_string(),
            role: "engineer".to_string(),
            repo: "acme/service".to_string(),
            artifact: Artifact {
                item: json!(1),
                kind: "issue".to_string(),
            },
            job_payload: json!({}),
        }
    }

    #[test]
    fn registered_worker_line_matches_observability_contract() {
        assert_eq!(
            registered_worker_line("basic-delivery-1", 2),
            "smith-worker: registered worker_id=basic-delivery-1 capabilities=2"
        );
    }

    #[test]
    fn assigned_job_line_matches_observability_contract() {
        assert_eq!(
            assigned_job_line(&assign()),
            "smith-worker: assigned job_id=job-123 role=engineer repo=acme/service"
        );
    }

    #[test]
    fn result_sent_line_formats_success_status() {
        let result = test_job_result(json!({
            "protocol_version": WORKER_PROTOCOL_VERSION,
            "worker_id": "worker-1",
            "job_id": "job-123",
            "status": ResultStatus::Success,
            "branch": Branch {
                name: "agent/pr-for-code-1".to_string(),
                head_sha: "abc123".to_string(),
            },
            "failure": null,
            "verdict": null,
            "body": null,
            "summary": null,
            "details": null,
        }));

        assert_eq!(
            result_sent_line(&result),
            "smith-worker: result sent job_id=job-123 status=success"
        );
    }

    #[test]
    fn result_sent_line_formats_failure_class() {
        let result = test_job_result(json!({
            "protocol_version": WORKER_PROTOCOL_VERSION,
            "worker_id": "worker-1",
            "job_id": "job-456",
            "status": ResultStatus::Failure,
            "branch": null,
            "failure": Failure {
                class: FailureClass::Permanent,
                message: "configured failure".to_string(),
            },
            "verdict": null,
            "body": null,
            "summary": null,
            "details": null,
        }));

        assert_eq!(
            result_sent_line(&result),
            "smith-worker: result sent job_id=job-456 status=failure(permanent)"
        );
    }

    #[test]
    fn result_sent_line_formats_failure_without_details_as_unknown() {
        let result = test_job_result(json!({
            "protocol_version": WORKER_PROTOCOL_VERSION,
            "worker_id": "worker-1",
            "job_id": "job-789",
            "status": ResultStatus::Failure,
            "branch": null,
            "failure": null,
            "verdict": null,
            "body": null,
            "summary": null,
            "details": null,
        }));

        assert_eq!(
            result_sent_line(&result),
            "smith-worker: result sent job_id=job-789 status=failure(unknown)"
        );
    }

    fn test_job_result(value: serde_json::Value) -> JobResult {
        serde_json::from_value(value).expect("test JobResult JSON matches worker protocol")
    }
}
