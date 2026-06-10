use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Output};

use tokio::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleGitIdentity {
    pub user: String,
    pub email: String,
    pub token: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceConfig {
    pub root: PathBuf,
    pub base_branch: String,
}

pub struct Workspace {
    path: PathBuf,
    base_branch: String,
    remote_url: String,
    identity: RoleGitIdentity,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("git command `{command}` failed (status {status}): {stderr}")]
    Git {
        command: String,
        status: String,
        stderr: String,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid utf-8 in git output: {0}")]
    Utf8(String),
    #[error("invalid repo `{0}`: expected owner/name")]
    InvalidRepo(String),
}

pub fn forgejo_remote_url(base_url: &str, repo: &str) -> Result<String, WorkspaceError> {
    validate_repo(repo)?;

    Ok(format!("{}/{}.git", base_url.trim_end_matches('/'), repo))
}

impl Workspace {
    pub fn new(
        config: &WorkspaceConfig,
        repo: &str,
        role: &str,
        identity: RoleGitIdentity,
        remote_url: impl Into<String>,
    ) -> Result<Self, WorkspaceError> {
        validate_repo(repo)?;

        Ok(Self {
            path: config.root.join(repo.replace('/', "__")).join(role),
            base_branch: config.base_branch.clone(),
            remote_url: remote_url.into(),
            identity,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn prepare(&self, work_branch: &str) -> Result<(), WorkspaceError> {
        if self.path.exists() {
            self.run_workspace_git(
                false,
                "git remote set-url origin <remote>".to_string(),
                vec![
                    OsString::from("remote"),
                    OsString::from("set-url"),
                    OsString::from("origin"),
                    OsString::from(self.remote_url.as_str()),
                ],
            )
            .await?;
        } else {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            self.run_git(
                None,
                true,
                "git clone --no-checkout <remote> <checkout>".to_string(),
                vec![
                    OsString::from("clone"),
                    OsString::from("--no-checkout"),
                    OsString::from(self.remote_url.as_str()),
                    self.path.as_os_str().to_os_string(),
                ],
            )
            .await?;
        }

        let refspec = format!(
            "+refs/heads/{}:refs/remotes/origin/{}",
            self.base_branch, self.base_branch
        );
        self.run_workspace_git(
            true,
            format!("git fetch origin {}", self.base_branch),
            vec![
                OsString::from("fetch"),
                OsString::from("origin"),
                OsString::from(refspec),
            ],
        )
        .await?;

        self.run_workspace_git(
            false,
            format!("git checkout -B {work_branch} origin/{}", self.base_branch),
            vec![
                OsString::from("checkout"),
                OsString::from("-B"),
                OsString::from(work_branch),
                OsString::from(format!("origin/{}", self.base_branch)),
            ],
        )
        .await?;

        Ok(())
    }

    pub async fn commit_all(&self, message: &str) -> Result<String, WorkspaceError> {
        self.run_workspace_git(
            false,
            "git add -A".to_string(),
            vec![OsString::from("add"), OsString::from("-A")],
        )
        .await?;

        self.run_workspace_git(
            false,
            "git commit -m <message>".to_string(),
            vec![
                OsString::from("commit"),
                OsString::from("-m"),
                OsString::from(message),
            ],
        )
        .await?;

        self.head_sha().await
    }

    pub async fn push_branch(&self, branch_name: &str) -> Result<String, WorkspaceError> {
        self.run_workspace_git(
            true,
            format!("git push origin HEAD:refs/heads/{branch_name}"),
            vec![
                OsString::from("push"),
                OsString::from("origin"),
                OsString::from(format!("HEAD:refs/heads/{branch_name}")),
            ],
        )
        .await?;

        self.head_sha().await
    }

    pub async fn head_sha(&self) -> Result<String, WorkspaceError> {
        let output = self
            .run_workspace_git(
                false,
                "git rev-parse HEAD".to_string(),
                vec![OsString::from("rev-parse"), OsString::from("HEAD")],
            )
            .await?;
        let stdout = String::from_utf8(output.stdout)
            .map_err(|error| WorkspaceError::Utf8(error.to_string()))?;

        Ok(stdout.trim().to_string())
    }

    async fn run_workspace_git(
        &self,
        include_remote_header: bool,
        command: String,
        args: Vec<OsString>,
    ) -> Result<Output, WorkspaceError> {
        self.run_git(Some(&self.path), include_remote_header, command, args)
            .await
    }

    async fn run_git(
        &self,
        current_dir: Option<&Path>,
        include_remote_header: bool,
        command: String,
        args: Vec<OsString>,
    ) -> Result<Output, WorkspaceError> {
        let mut git = Command::new("git");
        git.env("GIT_TERMINAL_PROMPT", "0")
            .arg("-c")
            .arg(format!("user.name={}", self.identity.user))
            .arg("-c")
            .arg(format!("user.email={}", self.identity.email));

        if include_remote_header {
            git.arg("-c").arg(format!(
                "http.extraheader=AUTHORIZATION: token {}",
                self.identity.token
            ));
        }

        if let Some(current_dir) = current_dir {
            git.arg("-C").arg(current_dir);
        }

        let output = git.args(args).output().await?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(WorkspaceError::Git {
                command: redact_secret(command, &self.identity.token),
                status: status_string(output.status),
                stderr: redact_secret(
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                    &self.identity.token,
                ),
            })
        }
    }
}

fn validate_repo(repo: &str) -> Result<(), WorkspaceError> {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        Err(WorkspaceError::InvalidRepo(repo.to_string()))
    } else {
        Ok(())
    }
}

fn status_string(status: ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| status.to_string(), |code| code.to_string())
}

fn redact_secret(text: String, secret: &str) -> String {
    if secret.is_empty() {
        text
    } else {
        text.replace(secret, "<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgejo_remote_url_accepts_owner_name_repo() {
        assert_eq!(
            forgejo_remote_url("http://localhost:3000", "ai/smith").expect("valid repo"),
            "http://localhost:3000/ai/smith.git"
        );
        assert_eq!(
            forgejo_remote_url("http://localhost:3000/", "ai/smith").expect("valid repo"),
            "http://localhost:3000/ai/smith.git"
        );
    }

    #[test]
    fn forgejo_remote_url_rejects_malformed_repo_names() {
        for repo in ["smith", "ai/", "/smith", "ai/smith/extra"] {
            let error =
                forgejo_remote_url("http://localhost:3000", repo).expect_err("invalid repo");
            match error {
                WorkspaceError::InvalidRepo(invalid) => assert_eq!(invalid, repo),
                other => panic!("unexpected error for {repo:?}: {other:?}"),
            }
        }
    }
}
