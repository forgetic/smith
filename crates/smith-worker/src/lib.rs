pub mod agent_runner;
pub mod client;
pub mod coding_executor;
pub mod config;
pub mod executor;
pub mod external_command_runner;
pub mod observability;
pub mod pi_agent_runner;
pub mod run;
pub mod worker_machine;
pub mod worker_shell;
pub mod workspace;

pub use agent_runner::{AgentRunError, AgentRunner, WorkspaceResult};
pub use coding_executor::{CodingExecutor, CodingExecutorConfig};
pub use config::{
    AgentAuthChoice, AgentSurface, CapabilitySpec, CodingSurface, ExecutorSelection, ParseOutcome,
    SmithAgentSurface, USAGE, WorkerConfig, WorkerParams, parse, role_identities_from_env,
};
pub use executor::{JobExecutor, JobOutcome, StubExecutor, job_result};
pub use external_command_runner::ExternalCommandRunner;
pub use observability::{assigned_job_line, registered_worker_line, result_sent_line};
pub use pi_agent_runner::PiAgentRunner;
pub use run::run_worker;
pub use worker_machine::{WorkerCompletion, WorkerMachine, WorkerRequest};
pub use workspace::{
    RoleGitIdentity, Workspace, WorkspaceConfig, WorkspaceError, forgejo_remote_url,
};
