pub mod client;
pub mod config;
pub mod executor;
pub mod run;
pub mod workspace;

pub use config::{CapabilitySpec, ParseOutcome, USAGE, WorkerConfig, parse};
pub use executor::{JobExecutor, JobOutcome, StubExecutor, job_result};
pub use run::run_worker;
pub use workspace::{
    RoleGitIdentity, Workspace, WorkspaceConfig, WorkspaceError, forgejo_remote_url,
};
