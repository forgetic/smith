use std::collections::BTreeSet;
use std::sync::Arc;

use temper_worker_protocol::{ErrorCode, JobResult, WorkerProtocolMessage};
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

                match client.send(&WorkerProtocolMessage::Result(result)).await? {
                    Some(WorkerProtocolMessage::Release(_)) | None => {}
                    Some(other) => eprintln!("smith-worker: unexpected result reply from daemon: {other:?}"),
                }
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
