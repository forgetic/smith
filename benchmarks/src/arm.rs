//! Frozen local commits build in the harness's private clone, never a worktree.
//! `build` keeps immutable arm binaries, their SHA-256s and toolchain identity;
//! `resolve_commit` refuses dirty sources or unknown commits before any write
//! (benchmarks.md, sections 3.2, 5.4 and 10). No login or agent is involved.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::guard::Guard;

const CLONE_MARKER: &[u8] = b"smith-bench owned clone, version 1\n";

/// One immutable executable emitted by cargo for a built arm.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArmBinary {
    /// A basename containing ASCII letters, digits, hyphens or underscores.
    pub name: String,
    /// Exactly 64 lowercase hexadecimal digits.
    pub sha256: String,
}

/// The recorded binary identity supplied by a build to subsequent attempts.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuiltArm {
    pub version: u32,
    /// The full 40-digit lowercase source commit, never an abbreviated ref.
    pub commit: String,
    /// The product's `smith` executable, which names the arm directory.
    pub sha256: String,
    pub toolchain: String,
    pub binaries: Vec<ArmBinary>,
}

/// One successful build's directory and immutable manifest, returned to its caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    pub directory: PathBuf,
    pub arm: BuiltArm,
}

fn hexadecimal(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Name an arm from its full commit and the product executable's SHA-256.
pub fn arm_name(commit: &str, sha256: &str) -> Result<String, String> {
    if !hexadecimal(commit, 40) || !hexadecimal(sha256, 64) {
        return Err("arm identity needs a full lowercase commit and SHA-256".into());
    }
    Ok(format!("{commit}-{sha256}"))
}

fn git(repository: &Path) -> Command {
    let mut command = Command::new("git");
    command.args(["-c", "core.hooksPath=/dev/null", "-c", "init.templateDir=", "-C"]).arg(repository);
    for name in
        ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES"]
    {
        command.env_remove(name);
    }
    command.env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_OPTIONAL_LOCKS", "0");
    command.stdin(Stdio::null());
    command
}

fn output(command: &mut Command, operation: &str) -> Result<String, String> {
    let result = command.output().map_err(|error| format!("{operation}: {error}"))?;
    if !result.status.success() {
        return Err(format!("{operation} failed with {}", result.status));
    }
    String::from_utf8(result.stdout).map_err(|_| format!("{operation} returned non-UTF-8 output"))
}

/// Resolve a local commit only when the source's index and working tree are clean.
/// Git arguments are passed directly; a commit string never becomes shell code.
pub fn resolve_commit(repository: &Path, reference: &str) -> Result<String, String> {
    let dirty =
        output(git(repository).args(["status", "--porcelain=v1", "--untracked-files=normal"]), "inspect source")?;
    if !dirty.is_empty() {
        return Err("source repository is dirty; commit or isolate changes before building an arm".into());
    }
    let commit = output(
        git(repository).args(["rev-parse", "--verify", "--end-of-options", &format!("{reference}^{{commit}}")]),
        "resolve commit",
    )?;
    let commit = commit.trim();
    if !hexadecimal(commit, 40) {
        return Err("unknown or unsupported commit".into());
    }
    Ok(commit.into())
}

/// Resolve the state root without writing; persistent build data stays outside git.
pub fn state_root(guard: &Guard) -> Result<PathBuf, String> {
    let state = std::env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()).map_or_else(
        || {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".local/state"))
                .ok_or("HOME or XDG_STATE_HOME is required")
        },
        |state| Ok(PathBuf::from(state)),
    )?;
    let root = guard.write_path(&state.join("smith-bench"))?;
    outside_repositories(&root)?;
    Ok(root)
}

fn outside_repositories(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        if ancestor.join(".git").exists() || (ancestor.join("HEAD").is_file() && ancestor.join("objects").is_dir()) {
            return Err("benchmark runtime data must be outside every repository".into());
        }
    }
    Ok(())
}

fn private_directory(guard: &Guard, path: &Path) -> Result<PathBuf, String> {
    let path = guard.write_path(path)?;
    fs::DirBuilder::new().recursive(true).mode(0o700).create(&path).map_err(|error| error.to_string())?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("build directories must be private (0700)".into());
    }
    Ok(path)
}

fn write_new(guard: &Guard, path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let path = guard.write_path(path)?;
    let mut file =
        OpenOptions::new().write(true).create_new(true).mode(mode).open(path).map_err(|error| error.to_string())?;
    file.write_all(bytes).and_then(|()| file.sync_all()).map_err(|error| error.to_string())
}

fn prepare_clone(guard: &Guard, root: &Path, source: &Path, commit: &str) -> Result<PathBuf, String> {
    let clone = guard.write_path(&root.join("clone"))?;
    let marker = guard.write_path(&root.join(".clone-owned"))?;
    if clone.exists() {
        if fs::read(&marker).map_err(|_| "existing clone has no harness ownership marker")? != CLONE_MARKER {
            return Err("existing clone is not owned by smith-bench".into());
        }
    } else {
        fs::DirBuilder::new().mode(0o700).create(&clone).map_err(|error| error.to_string())?;
        write_new(guard, &marker, CLONE_MARKER, 0o600)?;
    }
    private_directory(guard, &clone)?;
    output(git(&clone).args(["init", "--quiet"]), "initialize private clone")?;
    output(git(&clone).arg("fetch").args(["--quiet", "--no-tags"]).arg(source).arg(commit), "fetch local commit")?;
    output(git(&clone).args(["checkout", "--quiet", "--detach", commit]), "checkout frozen commit")?;
    let head = output(git(&clone).args(["rev-parse", "HEAD"]), "verify frozen checkout")?;
    if head.trim() != commit {
        return Err("private clone did not check out requested commit".into());
    }
    let status =
        output(git(&clone).args(["status", "--porcelain=v1", "--untracked-files=normal"]), "verify clean clone")?;
    if !status.is_empty() {
        return Err("private clone is dirty; refusing a contaminated build".into());
    }
    Ok(clone)
}

/// SHA-256 a regular binary in bounded chunks; symlinks are refused.
pub fn binary_digest(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err("arm executable must be a regular file".into());
    }
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut bytes = [0; 8192];
    loop {
        let read = file.read(&mut bytes).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&bytes[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn cargo_binaries(clone: &Path, target: &Path) -> Result<Vec<PathBuf>, String> {
    let mut command = Command::new("cargo");
    command
        .args([
            "build",
            "--release",
            "--locked",
            "--package",
            "smith",
            "--bins",
            "--message-format=json-render-diagnostics",
        ])
        .current_dir(clone)
        .env("CARGO_TARGET_DIR", target)
        .env("CARGO_BUILD_JOBS", "1")
        .stdin(Stdio::null())
        .stderr(Stdio::inherit());
    let records = output(&mut command, "cargo build --release --locked")?;
    let mut binaries = Vec::new();
    for line in records.lines() {
        let record: serde_json::Value =
            serde_json::from_str(line).map_err(|_| "cargo returned malformed build metadata")?;
        if record["reason"] == "compiler-artifact"
            && record["profile"]["test"] == false
            && let Some(executable) = record["executable"].as_str()
        {
            let executable = PathBuf::from(executable).canonicalize().map_err(|error| error.to_string())?;
            if !executable.starts_with(target) {
                return Err("cargo executable escaped the private target directory".into());
            }
            binaries.push(executable);
        }
    }
    binaries.sort();
    binaries.dedup();
    if binaries.is_empty() {
        return Err("cargo emitted no arm executable".into());
    }
    Ok(binaries)
}

fn record_arm(commit: &str, toolchain: String, executables: &[PathBuf]) -> Result<BuiltArm, String> {
    let mut binaries = Vec::new();
    for executable in executables {
        let name = executable.file_name().and_then(|name| name.to_str()).ok_or("binary has no UTF-8 name")?;
        if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) {
            return Err("unsafe arm executable name".into());
        }
        binaries.push(ArmBinary { name: name.into(), sha256: binary_digest(executable)? });
    }
    let sha256 = binaries
        .iter()
        .find(|binary| binary.name == "smith")
        .ok_or("arm has no smith product executable")?
        .sha256
        .clone();
    Ok(BuiltArm { version: 1, commit: commit.into(), sha256, toolchain, binaries })
}

fn verify_arm(directory: &Path, arm: &BuiltArm) -> Result<(), String> {
    let recorded: BuiltArm =
        serde_json::from_slice(&fs::read(directory.join("arm.json")).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    if &recorded != arm {
        return Err("existing arm metadata disagrees with this build; refusing overwrite".into());
    }
    for binary in &arm.binaries {
        if binary_digest(&directory.join(&binary.name))? != binary.sha256 {
            return Err("existing arm binary digest mismatch".into());
        }
    }
    Ok(())
}

fn archive(guard: &Guard, arms: &Path, executables: &[PathBuf], arm: &BuiltArm) -> Result<PathBuf, String> {
    let directory = guard.write_path(&arms.join(arm_name(&arm.commit, &arm.sha256)?))?;
    if directory.exists() {
        verify_arm(&directory, arm)?;
        return Ok(directory);
    }
    let temporary = guard.write_path(&arms.join(format!(".build-{}", std::process::id())))?;
    fs::DirBuilder::new().mode(0o700).create(&temporary).map_err(|error| error.to_string())?;
    let result = (|| {
        for (executable, binary) in executables.iter().zip(&arm.binaries) {
            let destination = guard.write_path(&temporary.join(&binary.name))?;
            fs::copy(executable, &destination).map_err(|error| error.to_string())?;
            fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
            if binary_digest(&destination)? != binary.sha256 {
                return Err("copied arm digest mismatch".into());
            }
        }
        let bytes = serde_json::to_vec_pretty(arm).map_err(|error| error.to_string())?;
        write_new(guard, &temporary.join("arm.json"), &bytes, 0o600)?;
        fs::rename(&temporary, &directory).map_err(|error| error.to_string())?;
        Ok(directory)
    })();
    if result.is_err() {
        fs::remove_dir_all(temporary).map_err(|error| format!("failed arm cleanup: {error}"))?;
    }
    result
}

/// Build and archive a clean local commit, called only inside a `heavy` task.
/// The source is read-only; every persistent destination passes the shared guard.
pub fn build(repository: &Path, reference: &str) -> Result<Build, String> {
    let repository = repository.canonicalize().map_err(|error| error.to_string())?;
    let commit = resolve_commit(&repository, reference)?;
    let guard = Guard::from_environment()?;
    let root = private_directory(&guard, &state_root(&guard)?)?;
    let clone = prepare_clone(&guard, &root, &repository, &commit)?;
    let target = private_directory(&guard, &root.join("build-target"))?;
    let arms = private_directory(&guard, &root.join("arms"))?;
    let toolchain =
        output(Command::new("rustc").args(["--version", "--verbose"]).current_dir(&clone), "record toolchain")?;
    let executables = cargo_binaries(&clone, &target)?;
    let arm = record_arm(&commit, toolchain, &executables)?;
    let directory = archive(&guard, &arms, &executables, &arm)?;
    Ok(Build { directory, arm })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_names_refuse_ambiguous_or_unsafe_identities() {
        let commit = "a".repeat(40);
        let digest = "0".repeat(64);
        assert_eq!(arm_name(&commit, &digest).expect("full identities"), format!("{commit}-{digest}"));
        for (commit, digest) in [("30259d8", digest.as_str()), ("../escape", digest.as_str()), (commit.as_str(), "bad")]
        {
            assert!(arm_name(commit, digest).is_err());
        }
    }

    #[test]
    fn dirty_and_unknown_commits_are_refused_before_any_build_write() {
        let root = std::env::temp_dir().join(format!("smith-bench-arm-source-{}", std::process::id()));
        fs::create_dir(&root).expect("isolated source");
        output(git(&root).args(["init", "--quiet"]), "test git init").expect("init");
        output(
            git(&root).args([
                "-c",
                "user.name=Benchmark test",
                "-c",
                "user.email=benchmark@example.invalid",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "synthetic seed",
            ]),
            "test commit",
        )
        .expect("commit");
        let commit = resolve_commit(&root, "HEAD").expect("clean commit");
        assert_eq!(commit.len(), 40);
        assert!(resolve_commit(&root, "unknown-commit").is_err());
        assert!(resolve_commit(&root, "--help").is_err());
        fs::write(root.join("dirty"), "synthetic untracked change").expect("dirty source");
        assert!(resolve_commit(&root, "HEAD").expect_err("dirty source").contains("dirty"));
        assert!(outside_repositories(&root.join("private/arms")).is_err());
        fs::remove_dir_all(root).expect("remove only the synthetic repository");
    }
}
