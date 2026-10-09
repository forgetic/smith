//! Opt-in shipped-binary stories against real backends (testing.md, section 2.3).
//! The `live` nextest profile selects this binary; the merge gate excludes it.

#[path = "support/live.rs"]
pub mod live;
pub mod support;

#[test]
fn bootstrap_settings() {
    if !live::enabled() {
        return;
    }
    let Some(directory) = live::setting("SMITH_TEST_LIVE_BOOTSTRAP_DIR") else {
        return;
    };
    let directory =
        smith_bench::guard::write_path(std::path::Path::new(&directory)).expect("guarded bootstrap directory");
    assert!(directory.is_absolute(), "SMITH_TEST_LIVE_BOOTSTRAP_DIR must be absolute");
    let environment = live::Environment::load(true);
    std::fs::create_dir_all(&directory).expect("bootstrap settings directory");
    for backend in environment.backends {
        let state = smith_bench::guard::write_path(&directory.join(&backend.name)).expect("guarded bootstrap state");
        std::fs::create_dir_all(&state).expect("bootstrap state directory");
        std::fs::write(
            smith_bench::guard::write_path(&state.join("settings.json")).expect("guarded bootstrap settings"),
            serde_json::to_vec_pretty(&backend.settings(&state, false)).expect("bootstrap JSON"),
        )
        .expect("bootstrap settings");
    }
}

use smith_local_process_world::referee::CheckoutRead;
use smith_real_world::Scratch;
use support::live_run::{Expectation, run};

const CHANGE: &str = "In repository repo, read original.txt, replace its content with exactly edited followed by one newline, create result.txt containing exactly new followed by one newline, and use the command tool to create command.txt containing exactly ran followed by one newline. Run the configured .smith-test/check checks without modifying that script. Finish with a change result, a short title and body, so the host delivers the checked commit. Do not run git commit or push yourself.";

fn changed_files() -> Vec<(Vec<u8>, Vec<u8>)> {
    [("original.txt", "edited\n"), ("result.txt", "new\n"), ("command.txt", "ran\n"), ("checks-ran.txt", "checked\n")]
        .into_iter()
        .map(|(path, content)| (path.as_bytes().to_vec(), content.as_bytes().to_vec()))
        .collect()
}

#[expect(
    clippy::disallowed_types,
    reason = "10-product replaces the live command stories and their independent command checks"
)]
fn change(backend: &live::Backend, scratch: &Scratch, prompt: &str, extra: Option<(Vec<u8>, Vec<u8>)>) {
    let mut files = changed_files();
    if let Some(extra) = extra {
        files.push(extra);
    }
    run(scratch, prompt, true, Expectation { before: scratch.checkout().head(), files });
    assert!(
        std::process::Command::new("/bin/sh")
            .arg(".smith-test/check")
            .current_dir(scratch.path().join("repo"))
            .status()
            .expect("independent check execution")
            .success(),
        "prescribed check passes independently"
    );
    let tokens =
        smith_local_shell::local_tokens::Tokens::new(&backend.tokens, smith_local_shell::local_host::token_limits())
            .expect("private test token store")
            .load(0)
            .expect("read test grant")
            .expect("durable grant");
    let trace = std::fs::read(scratch.path().join("agent-trace.jsonl")).expect("outside trace");
    for secret in [Some(tokens.access_token.as_ref()), tokens.refresh_token.as_deref()].into_iter().flatten() {
        assert!(
            !secret.is_empty() && !trace.windows(secret.len()).any(|bytes| bytes == secret),
            "trace contains no credential value"
        );
    }
}

#[test]
fn a_run_refreshes_its_grant_at_the_issuer() {
    if !live::enabled() {
        return;
    }
    let environment = live::Environment::load(false);
    for backend in environment.backends {
        eprintln!("live refresh provider={}", backend.name);
        let store = smith_local_shell::local_tokens::Tokens::new(
            &backend.tokens,
            smith_local_shell::local_host::token_limits(),
        )
        .expect("dedicated token store");
        let mut saved = store.load(0).expect("read dedicated grant").expect("sign in by hand first");
        assert!(saved.refresh_token.as_ref().is_some_and(|token| !token.is_empty()), "dedicated test grant must support refresh");
        let generation = saved.generation;
        saved.expires_at = skein_lib::Wall::EPOCH;
        store
            .save(
                0,
                &skein_oauth::encode_record(&saved, &smith_local_shell::local_host::token_limits())
                    .expect("dedicated expired record"),
            )
            .expect("force only the test-owned grant to refresh");
        let scratch = Scratch::new();
        backend.configure(&scratch, false, None);
        run(
            &scratch,
            "Give a brief greeting and call finish with a report result.",
            false,
            Expectation { before: None, files: vec![] },
        );
        let refreshed = store.load(0).expect("durable refreshed record").expect("refreshed grant persisted");
        assert!(refreshed.generation > generation, "real issuer exchange advanced durable token generation");
        assert!(
            refreshed.expires_at > skein_shell::Clock::new().now().wall && !refreshed.access_token.is_empty(),
            "the refreshed grant is usable and persists for the next run"
        );
    }
}

#[test]
fn a_chat_ends_with_a_commit_in_place() {
    if !live::enabled() {
        return;
    }
    for backend in live::Environment::load(false).backends {
        eprintln!("live commit provider={}", backend.name);
        let scratch = Scratch::new();
        backend.configure(&scratch, true, None);
        change(&backend, &scratch, CHANGE, None);
    }
}

fn turns(scratch: &Scratch) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(scratch.path().join("chat"))
        .expect("durable chat files")
        .map(|entry| entry.expect("turn directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "turn"))
        .map(|path| {
            let bytes = std::fs::read(&path).expect("durable turn");
            (path, bytes)
        })
        .collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

#[test]
fn a_second_run_resumes_the_chat_from_its_files() {
    if !live::enabled() {
        return;
    }
    for backend in live::Environment::load(false).backends {
        eprintln!("live durable resume provider={}", backend.name);
        let scratch = Scratch::new();
        let marker =
            format!("smith-live-memory-{}-{}", std::process::id(), skein_shell::Clock::new().now().now.as_nanos());
        backend.configure(&scratch, false, None);
        run(
            &scratch,
            &format!(
                "Remember this code for our next invocation: {marker}. Do not write files. Call finish with a short report result."
            ),
            false,
            Expectation { before: None, files: vec![] },
        );
        let before = turns(&scratch);
        assert!(!before.is_empty(), "first binary wrote durable turns");
        assert!(
            !scratch.path().join("repo/remembered.txt").exists(),
            "the first invocation keeps the code only in conversation history"
        );
        backend.configure(&scratch, true, None);
        let prompt = format!(
            "{CHANGE} Also create remembered.txt containing exactly the code I asked you to remember in the previous invocation, with no newline. Obtain it from the saved conversation, not from an invented value."
        );
        change(&backend, &scratch, &prompt, Some((b"remembered.txt".to_vec(), marker.into_bytes())));
        let after = turns(&scratch);
        assert!(after.len() > before.len(), "second binary appended durable history");
        assert!(before.iter().all(|entry| after.contains(entry)), "earlier durable turns remain unchanged");
    }
}

#[expect(clippy::disallowed_types, reason = "10-product replaces the live command stories and their remote checks")]
fn git(path: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    std::process::Command::new("/usr/bin/git").arg("-C").arg(path).args(arguments).output().expect("live git command")
}

#[test]
fn a_configured_push_lands() {
    if !live::enabled() {
        return;
    }
    let environment = live::Environment::load(false);
    let Some(remote) = environment.remote else {
        eprintln!("configured push not run: SMITH_TEST_LIVE_GIT_REMOTE is absent");
        return;
    };
    for backend in environment.backends {
        let scratch = Scratch::new();
        let repo = scratch.path().join("repo");
        let branch = format!(
            "smith-live-{}-{}-{}",
            backend.name,
            std::process::id(),
            skein_shell::Clock::new().now().now.as_nanos()
        );
        assert!(git(&repo, &["checkout", "-q", "-b", &branch]).status.success(), "create unique live push branch");
        assert!(
            git(&repo, &["remote", "add", "live", &remote]).status.success(),
            "configure caller-selected test remote"
        );
        backend.configure(&scratch, true, Some(("live", &branch)));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            change(&backend, &scratch, CHANGE, None);
            let reference = format!("refs/heads/{branch}");
            let seen = git(&repo, &["ls-remote", "live", &reference]);
            assert!(seen.status.success(), "inspect configured remote branch; diagnostics withheld");
            let head = scratch.checkout().head().expect("delivered head");
            assert!(
                seen.stdout.split(u8::is_ascii_whitespace).next() == Some(head.as_slice()),
                "configured push lands at the delivered commit"
            );
        }));
        let reference = format!(":refs/heads/{branch}");
        let cleanup = git(&repo, &["push", "live", &reference]);
        assert!(cleanup.status.success(), "remove live test branch even when its story fails; diagnostics withheld");
        if let Err(failure) = outcome {
            std::panic::resume_unwind(failure);
        }
    }
}
