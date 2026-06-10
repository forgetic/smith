use std::collections::BTreeSet;

use reqwest::Url;
use temper_worker_protocol::{
    Capability, Capacity, Heartbeat, HeartbeatState, JobHeartbeat, Poll, Register,
    WORKER_PROTOCOL_VERSION, WorkerProtocolMessage,
};
use thiserror::Error;

use crate::config::WorkerConfig;

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("invalid daemon URL `{url}`: {message}")]
    InvalidDaemonUrl { url: String, message: String },
    #[error("request to {endpoint} failed: {source}")]
    Transport {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("daemon returned HTTP {status} from {endpoint}: {body}")]
    HttpStatus {
        endpoint: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error(
        "daemon response from {endpoint} was not valid worker protocol JSON: {source}; body: {body}"
    )]
    Decode {
        endpoint: String,
        body: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("worker result channel closed before completion")]
    ResultChannelClosed,
}

pub struct WorkerClient {
    http: reqwest::Client,
    endpoint: String,
}

impl WorkerClient {
    pub fn new(daemon_url: &str) -> Result<Self, WorkerError> {
        let trimmed = daemon_url.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            return Err(WorkerError::InvalidDaemonUrl {
                url: daemon_url.to_string(),
                message: "URL must not be empty".to_string(),
            });
        }

        let base = Url::parse(trimmed).map_err(|error| WorkerError::InvalidDaemonUrl {
            url: daemon_url.to_string(),
            message: error.to_string(),
        })?;
        match base.scheme() {
            "http" | "https" => {}
            scheme => {
                return Err(WorkerError::InvalidDaemonUrl {
                    url: daemon_url.to_string(),
                    message: format!("unsupported URL scheme `{scheme}`"),
                });
            }
        }
        if base.host_str().is_none() {
            return Err(WorkerError::InvalidDaemonUrl {
                url: daemon_url.to_string(),
                message: "URL must include a host".to_string(),
            });
        }

        let endpoint = format!("{trimmed}/v1/message");
        Url::parse(&endpoint).map_err(|error| WorkerError::InvalidDaemonUrl {
            url: daemon_url.to_string(),
            message: error.to_string(),
        })?;

        Ok(Self {
            http: reqwest::Client::new(),
            endpoint,
        })
    }

    pub async fn send(
        &self,
        msg: &WorkerProtocolMessage,
    ) -> Result<Option<WorkerProtocolMessage>, WorkerError> {
        let response = self
            .http
            .post(&self.endpoint)
            .json(msg)
            .send()
            .await
            .map_err(|source| WorkerError::Transport {
                endpoint: self.endpoint.clone(),
                source,
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|source| WorkerError::Transport {
                endpoint: self.endpoint.clone(),
                source,
            })?;
        let body = String::from_utf8_lossy(&bytes).into_owned();

        if status != reqwest::StatusCode::OK {
            return Err(WorkerError::HttpStatus {
                endpoint: self.endpoint.clone(),
                status,
                body,
            });
        }

        if body.trim().is_empty() {
            return Ok(None);
        }

        let message = serde_json::from_slice(&bytes).map_err(|source| WorkerError::Decode {
            endpoint: self.endpoint.clone(),
            body,
            source,
        })?;
        Ok(Some(message))
    }
}

pub fn register_message(config: &WorkerConfig) -> WorkerProtocolMessage {
    WorkerProtocolMessage::Register(Register {
        protocol_version: WORKER_PROTOCOL_VERSION,
        worker_id: config.worker_id.clone(),
        capabilities: config
            .capabilities
            .iter()
            .map(|capability| Capability {
                role: capability.role.clone(),
                repo: capability.repo.clone(),
            })
            .collect(),
        capacity: Capacity {
            max_concurrent_jobs: config.max_concurrent_jobs,
        },
        labels: None,
    })
}

pub fn poll_message(config: &WorkerConfig, free_capacity: u32) -> WorkerProtocolMessage {
    WorkerProtocolMessage::Poll(Poll {
        protocol_version: WORKER_PROTOCOL_VERSION,
        worker_id: config.worker_id.clone(),
        free_capacity,
        max_wait_ms: Some(duration_millis(config.poll_wait)),
    })
}

pub fn heartbeat_message(
    config: &WorkerConfig,
    in_flight: &BTreeSet<String>,
    free_capacity: u32,
) -> WorkerProtocolMessage {
    WorkerProtocolMessage::Heartbeat(Heartbeat {
        protocol_version: WORKER_PROTOCOL_VERSION,
        worker_id: config.worker_id.clone(),
        jobs: in_flight
            .iter()
            .map(|job_id| JobHeartbeat {
                job_id: job_id.clone(),
                state: HeartbeatState::Running,
                message: "running".to_string(),
            })
            .collect(),
        free_capacity: Some(free_capacity),
    })
}

fn duration_millis(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::config::CapabilitySpec;

    use super::*;

    fn config() -> WorkerConfig {
        WorkerConfig {
            daemon_url: "http://127.0.0.1:1234".to_string(),
            worker_id: "worker-1".to_string(),
            capabilities: vec![
                CapabilitySpec {
                    repo: "ai/temper".to_string(),
                    role: "coder".to_string(),
                },
                CapabilitySpec {
                    repo: "ai/smith".to_string(),
                    role: "engineer".to_string(),
                },
            ],
            max_concurrent_jobs: 2,
            poll_wait: Duration::from_millis(1_500),
            heartbeat_interval: Duration::from_millis(500),
            executor: crate::config::ExecutorSelection::Stub,
        }
    }

    #[test]
    fn protocol_message_helpers_fill_version_and_config_fields() {
        let config = config();

        match register_message(&config) {
            WorkerProtocolMessage::Register(register) => {
                assert_eq!(register.protocol_version, WORKER_PROTOCOL_VERSION);
                assert_eq!(register.worker_id, "worker-1");
                assert_eq!(register.capacity.max_concurrent_jobs, 2);
                assert_eq!(register.capabilities.len(), 2);
                assert_eq!(register.capabilities[0].repo, "ai/temper");
            }
            other => panic!("unexpected message: {other:?}"),
        }

        match poll_message(&config, 1) {
            WorkerProtocolMessage::Poll(poll) => {
                assert_eq!(poll.protocol_version, WORKER_PROTOCOL_VERSION);
                assert_eq!(poll.worker_id, "worker-1");
                assert_eq!(poll.free_capacity, 1);
                assert_eq!(poll.max_wait_ms, Some(1_500));
            }
            other => panic!("unexpected message: {other:?}"),
        }

        let in_flight = BTreeSet::from(["job-a".to_string(), "job-b".to_string()]);
        match heartbeat_message(&config, &in_flight, 0) {
            WorkerProtocolMessage::Heartbeat(heartbeat) => {
                assert_eq!(heartbeat.protocol_version, WORKER_PROTOCOL_VERSION);
                assert_eq!(heartbeat.worker_id, "worker-1");
                assert_eq!(heartbeat.free_capacity, Some(0));
                assert_eq!(heartbeat.jobs.len(), 2);
                assert_eq!(heartbeat.jobs[0].job_id, "job-a");
                assert_eq!(heartbeat.jobs[0].state, HeartbeatState::Running);
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }
}
