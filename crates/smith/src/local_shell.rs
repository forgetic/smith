//! Local host command and shell effects (protocol/hosts.md, sections 5.1–5.6).
//! The shell owns paths, file descriptors and child launch. The local service
//! owns chat and run decisions. One Skein kernel loop drives terminal input,
//! signals, the hosted agent and their deadlines.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use skein_io::{self as io, kernel::Fd};
use skein_lib::Duration;
use skein_shell::{Clock, Config as KernelConfig, Kernel, Now, Wait};
use smith_host_domain as host;
use smith_host_protocol as protocol;
use smith_local_domain as local;
use smith_local_service as service;

use crate::{config, local_settings, local_store};

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
    let mut store = local_store::Store::new(chat_path, endpoints.clone(), 1 << 20)
        .map_err(|error| format!("chat store: {error}"))?;
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
    let worst = service::worst_case(&limits).ok_or("local memory calculation overflowed")?;
    let seed = skein_shell::seed().map_err(|error| format!("local seed failed (errno {error})"))?;
    let mut local_service = service::Service::new(
        service::Config {
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
        },
        seed,
    )
    .map_err(|error| format!("local service: {error:?}"))?;
    local_service
        .adopt_delivery_roots(
            delivery_roots.into(),
            settings.delivery_environment.iter().map(|entry| entry.as_bytes().into()).collect(),
        )
        .map_err(|error| format!("delivery directories: {error:?}"))?;
    let signals =
        skein_shell::open_termination_signals().map_err(|error| format!("local signals failed (errno {error})"))?;
    local_service
        .adopt_terminal(Fd::new(0), signals)
        .map_err(|fd| format!("local terminal {} cannot be adopted", fd.raw()))?;
    let operations = io::operations(&process_limits.io)
        .and_then(|count| count.checked_add(64))
        .ok_or("local IO operation count overflowed")?;
    let mut kernel = Kernel::open(KernelConfig { operations }).map_err(|error| format!("local kernel: {error}"))?;
    let clock = Clock::new();
    eprintln!("smith: local host started; worst case {worst} bytes");
    loop {
        kernel.reap(local_service.completions());
        let Now { now, wall } = clock.now();
        service::iterate(&mut local_service, now, wall);
        if let Some(exit) = settle_shell(&mut local_service, &mut store)? {
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

fn settle_shell(
    local_service: &mut service::Service,
    store: &mut local_store::Store,
) -> Result<Option<local::ExitStatus>, String> {
    let mut stdout = std::io::stdout().lock();
    for _ in 0..local_service.output().capacity() {
        let Some(text) = local_service.output().pop() else { break };
        stdout.write_all(&text).map_err(|error| format!("terminal write: {error}"))?;
    }
    stdout.flush().map_err(|error| format!("terminal flush: {error}"))?;
    for _ in 0..local_service.shell_requests().capacity() {
        let Some(request) = local_service.shell_requests().pop() else { break };
        match request {
            local::Request::Load => match store.load() {
                Ok(event) => local_service.local_event(event),
                Err(reason) => local_service.local_event(local::Event::StoreFailed { reason }),
            },
            local::Request::SaveState { state, fresh } => match store.save_state(state, fresh) {
                Ok(event) => local_service.local_event(event),
                Err(reason) => local_service.local_event(local::Event::StoreFailed { reason }),
            },
            local::Request::SaveTurn { number, read, turn } => match store.save_turn(number, read, turn) {
                Ok(event) => local_service.local_event(event),
                Err(reason) => local_service.local_event(local::Event::StoreFailed { reason }),
            },
            local::Request::SaveDelivery { record } => match store.save_delivery(&record) {
                Ok(event) => local_service.local_event(event),
                Err(reason) => local_service.local_event(local::Event::StoreFailed { reason }),
            },
            local::Request::Credential { account } => local_service
                .local_event(local::Event::NoCredential { account, reason: local::CredentialFailure::Missing }),
            local::Request::Git { owner, .. } => local_service.local_event(local::Event::Git {
                owner,
                result: local::GitResult::Failed {
                    reason: smith_domain::run::DeliveryReason::Broken,
                    diagnostic: Box::new(smith_domain::run::Diagnostic::empty()),
                },
            }),
            local::Request::PlainStatus { .. } => {
                local_service.local_event(local::Event::StoreFailed { reason: local::StoreFailure::Read })
            }
            local::Request::Exit { status } => return Ok(Some(status)),
            local::Request::Show { .. } | local::Request::Agent(_) | local::Request::External(_) => {
                unreachable!("service translates these before shell routing")
            }
        }
    }
    Ok(None)
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
