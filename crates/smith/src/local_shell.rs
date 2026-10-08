//! Local host command and shell effects (protocol/hosts.md, sections 5.1–5.6).
//! The shell owns paths, file descriptors and child launch. The local service
//! owns chat and run decisions. One Skein kernel loop drives terminal input,
//! signals, the hosted agent and their deadlines.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use skein_io::kernel::Fd;
use skein_lib::Duration;
use skein_shell::{Clock, Config as KernelConfig, Kernel, Now, Wait};
use skein_world::Host;
use smith_host_domain as host;
use smith_host_protocol as protocol;
use smith_local_domain as local;
use smith_local_service as service;

use crate::{
    config,
    local_host::{Local, Resources},
    local_settings,
};

pub fn run(settings_path: &Path, state_root: &Path, workspace_settings: Option<&Path>) -> Result<(), String> {
    let settings = local_settings::read(settings_path, workspace_settings)?;
    if settings.delivery_environment.len() > 64
        || settings.delivery_environment.iter().map(String::len).sum::<usize>() > 4096
        || settings.delivery_environment.iter().any(|entry| !entry.contains('=') || entry.contains('\0'))
    {
        return Err("delivery environment exceeds its bounds or has invalid entries".into());
    }
    let mut delivery_roots = Vec::with_capacity(settings.directories.len());
    for directory in &settings.directories {
        delivery_roots.push(
            skein_shell::open_root(Path::new(&directory.path))
                .map_err(|error| format!("delivery root {}: {error}", directory.path))?,
        );
    }
    let agent_bytes =
        serde_json::to_vec(&settings.agent).map_err(|error| format!("agent configuration JSON: {error}"))?;
    let agent_config = config::parse(&agent_bytes)?;
    let endpoints = agent_config.service.channel_endpoints.clone();
    let prepared = local_settings::policy(&settings, &endpoints, agent_config.service.limits.domain)?;
    let charter = local_settings::charter(&prepared.config, &endpoints)?;
    fs::create_dir_all(state_root).map_err(|error| format!("state directory: {error}"))?;
    let agent_path = state_root.join("agent.json");
    save_agent_config(&agent_path, &agent_bytes)?;
    let chat_path = state_root.join(&settings.chat);
    let token_path = token_directory(settings.token_directory.as_deref())?;
    let parent = token_path.parent().ok_or("token directory has no parent")?;
    fs::create_dir_all(parent).map_err(|error| format!("token directory parent: {error}"))?;
    let mut effect_roots = Vec::new();
    if settings.in_process {
        for directory in &settings.directories {
            effect_roots.push(
                skein_shell::open_root(Path::new(&directory.path))
                    .map_err(|error| format!("agent root {}: {error}", directory.path))?,
            );
        }
    }
    let launch_root = skein_shell::open_root(Path::new(".")).map_err(|error| format!("launch root: {error}"))?;
    let executable = std::env::current_exe().map_err(|error| format!("agent program path: {error}"))?;
    let host_limits = host_limits();
    let queue = local::max_out(&prepared.limits).max(host::max_out(&host_limits)).max(256);
    let process_limits = service::ProcessLimits {
        io: agent_config.service.limits.io,
        channel: protocol::Limits {
            bodies: agent_config.service.limits.channel.bodies,
            channel: agent_config.service.limits.channel.channel,
            calls: host_limits.calls,
        },
        detail_bytes: host_limits.detail_bytes,
        queue,
    };
    let limits = service::Limits { local: prepared.limits, host: host_limits, process: process_limits, queue };
    let seed = skein_shell::seed().map_err(|error| format!("local seed failed (errno {error})"))?;
    let local_config = service::Config {
        local: prepared.config,
        limits,
        charter,
        endpoints,
        paths: prepared.paths,
        launch: service::Launch {
            program: executable.as_os_str().as_bytes().into(),
            arguments: Box::new([Box::from(&b"agent"[..]), agent_path.as_os_str().as_bytes().into()]),
            environment: Box::new([]),
            root: launch_root,
            directory: Box::from(&b"."[..]),
        },
    };
    let signals =
        skein_shell::open_termination_signals().map_err(|error| format!("local signals failed (errno {error})"))?;
    let lower = settings.in_process.then_some(agent_config.service);
    let mut local_service = Local::new(
        local_config,
        lower,
        Resources {
            state_directory: chat_path,
            token_directory: token_path,
            input: Fd::new(0),
            output: Fd::new(1),
            signals,
            delivery_roots: delivery_roots.into(),
            effect_roots: effect_roots.into(),
            delivery_environment: settings.delivery_environment.iter().map(|entry| entry.as_bytes().into()).collect(),
            accounts: settings.accounts.into(),
            seed,
        },
    )?;
    let worst = local_service.worst_case();
    let operations = local_service.operations();
    let mut kernel = Kernel::open(KernelConfig { operations }).map_err(|error| format!("local kernel: {error}"))?;
    let clock = Clock::new();
    eprintln!("smith: local host started; worst case {worst} bytes");
    loop {
        kernel.reap(local_service.completions());
        let Now { now, wall } = clock.now();
        local_service.iterate(now, wall);
        if let Some(exit) = local_service.result() {
            let exit = exit.map_err(str::to_owned)?;
            return match exit {
                local::ExitStatus::Success => Ok(()),
                local::ExitStatus::Failed => Err("local chat ended with a failure".into()),
            };
        }
        let wait = if local_service.work_pending(now) {
            Wait::No
        } else {
            match local_service.next_deadline() {
                Some(deadline) => Wait::Until(deadline),
                None => Wait::Forever,
            }
        };
        kernel.submit(local_service.submissions(), wait);
    }
}

fn save_agent_config(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("agent configuration has no directory")?;
    let temporary = parent.join(".agent.json.tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)
        .map_err(|error| format!("agent configuration open: {error}"))?;
    file.write_all(bytes).map_err(|error| format!("agent configuration write: {error}"))?;
    file.sync_all().map_err(|error| format!("agent configuration sync: {error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("agent configuration rename: {error}"))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("agent configuration directory sync: {error}"))
}

fn token_directory(configured: Option<&str>) -> Result<std::path::PathBuf, String> {
    if let Some(configured) = configured {
        return Ok(configured.into());
    }
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(base) => std::path::PathBuf::from(base),
        None => std::path::PathBuf::from(std::env::var_os("HOME").ok_or("HOME is absent; configure token_directory")?)
            .join(".config"),
    };
    Ok(base.join("smith/tokens"))
}

fn host_limits() -> host::Limits {
    host::Limits {
        agents: 1,
        directories: 2,
        conflicts: 64,
        path_bytes: 4096,
        name_bytes: 256,
        accounts: 4,
        charter_bytes: 262_144,
        transcript_bytes: 33_554_432,
        answered_bytes: 65_536,
        message_bytes: 4096,
        messages: 8,
        calls: 16,
        call_bytes: 65_536,
        answer_bytes: host::Delivered::worst_case().max(65_536),
        turns: 64,
        turn_bytes: 262_144,
        unacknowledged_bytes: 33_554_432,
        fact_bytes: 4096,
        outcome_bytes: 4096,
        detail_bytes: 4096,
        spawn_timeout: Duration::from_secs(20),
        no_progress: Duration::from_secs(300),
        long_span: Duration::from_secs(3600),
        wall_time: Duration::from_secs(3600),
        grace: Duration::from_secs(5),
        kill_after: Duration::from_secs(2),
        facts: 1024,
    }
}
