use crate::{Git, GitAction, GitCompletion, GitLimits};
use alloc::boxed::Box;
use smith_domain::run;
use smith_local_domain::{GitOp, GitResult};

fn limits() -> GitLimits {
    GitLimits { argument_bytes: 4096, output_bytes: 64, conflicts: 2, path_bytes: 64, detail_bytes: 32 }
}

fn command(code: Option<u8>, stdout: &[u8], timed_out: bool) -> GitCompletion {
    GitCompletion::Command { code, stdout: stdout.into(), stderr: Box::new([]), timed_out }
}

#[test]
fn a_commit_deadline_or_output_overflow_is_uncertain_and_never_reissued() {
    for (stdout, timed_out) in [(b"".as_slice(), true), ([b'x'; 65].as_slice(), false)] {
        let (mut git, _) = Git::new(GitOp::Commit { message: b"Title".as_slice().into() }, limits()).unwrap();
        let GitAction::Command { .. } = git.complete(command(Some(0), b"", false)) else { panic!("commit command") };
        let GitAction::Done(GitResult::Uncertain { .. }) = git.complete(command(Some(0), stdout, timed_out)) else {
            panic!("uncertain terminal")
        };
    }
}

#[test]
fn an_unreadable_head_after_a_successful_commit_is_uncertain() {
    let (mut git, _) = Git::new(GitOp::Commit { message: b"Title".as_slice().into() }, limits()).unwrap();
    git.complete(command(Some(0), b"", false));
    git.complete(command(Some(0), b"", false));
    let GitAction::Done(GitResult::Uncertain { .. }) = git.complete(command(Some(1), b"", false)) else {
        panic!("uncertain terminal")
    };
}

#[test]
fn failed_staging_has_no_commit_effect() {
    let (mut git, _) = Git::new(GitOp::Commit { message: b"Title".as_slice().into() }, limits()).unwrap();
    let GitAction::Done(GitResult::NoEffect { .. }) = git.complete(command(Some(1), b"", false)) else {
        panic!("no effect terminal")
    };
}

#[test]
fn an_absent_trailer_reads_the_unchanged_head_as_no_named_commit() {
    let name = run::CallName { activation: 7, completion: 3, position: 1 };
    let (mut git, _) = Git::new(GitOp::Inspect { name }, limits()).unwrap();
    let GitAction::Command { args } = git.complete(command(Some(0), b"", false)) else { panic!("head inspection") };
    assert_eq!(args[0].as_ref(), b"rev-parse");
    let GitAction::Done(GitResult::Inspected { head, named: false }) =
        git.complete(command(Some(0), b"01234567\n", false))
    else {
        panic!("absent inspection")
    };
    assert_eq!(head.as_ref(), b"01234567");
}

#[test]
fn a_trailer_with_a_longer_position_does_not_name_this_delivery() {
    let name = run::CallName { activation: 7, completion: 3, position: 1 };
    let output = b"01234567\0Title\n\nSmith-Delivery: 7/3/10\n";
    let (mut git, _) = Git::new(GitOp::Inspect { name }, limits()).unwrap();
    let GitAction::Done(GitResult::Inspected { named: false, .. }) = git.complete(command(Some(0), output, false))
    else {
        panic!("longer name is not this delivery")
    };
}

#[test]
fn marker_reads_visit_original_paths_in_order_and_stop_at_the_first_marker() {
    let owner = skein_lib::Token::new(17);
    let root = skein_lib::Token::new(19);
    let paths = Box::new([
        Box::from(b"first.rs".as_slice()),
        Box::from(b"second.rs".as_slice()),
        Box::from(b"third.rs".as_slice()),
    ]);
    let mut marker_limits = limits();
    marker_limits.conflicts = 3;
    let mut markers = crate::Markers::new(owner, root, paths, skein_lib::Time::from_nanos(50), marker_limits).unwrap();
    let crate::MarkerAction::File { request: skein_io::file::Request::Load { path, no_follow: true, .. }, .. } =
        markers.start()
    else {
        panic!("first original path")
    };
    assert_eq!(path.as_ref(), b"first.rs");
    let crate::MarkerAction::File { request: skein_io::file::Request::Load { path, .. }, .. } =
        markers.from_file(skein_io::file::Event::Loaded { owner, bytes: b"clean\n".as_slice().into() })
    else {
        panic!("second original path")
    };
    assert_eq!(path.as_ref(), b"second.rs");
    let crate::MarkerAction::Done(GitResult::Markers { first: Some(path) }) =
        markers.from_file(skein_io::file::Event::Loaded {
            owner,
            bytes: b"<<<<<<< ours\nchanged\n=======\n".as_slice().into(),
        })
    else {
        panic!("first remaining marker")
    };
    assert_eq!(path.as_ref(), b"second.rs");
}

#[test]
fn a_deleted_original_conflict_file_is_resolved() {
    let owner = skein_lib::Token::new(17);
    let mut markers = crate::Markers::new(
        owner,
        skein_lib::Token::new(19),
        Box::new([Box::from(b"removed.rs".as_slice())]),
        skein_lib::Time::from_nanos(50),
        limits(),
    )
    .unwrap();
    drop(markers.start());
    let crate::MarkerAction::Done(GitResult::Markers { first: None }) =
        markers.from_file(skein_io::file::Event::Failed { owner, error: skein_io::kernel::Error::NotFound })
    else {
        panic!("deleted conflict resolved")
    };
}

#[test]
fn a_marker_read_over_its_bound_is_failed_instead_of_declared_clean() {
    let owner = skein_lib::Token::new(17);
    let mut markers = crate::Markers::new(
        owner,
        skein_lib::Token::new(19),
        Box::new([Box::from(b"large.rs".as_slice())]),
        skein_lib::Time::from_nanos(50),
        limits(),
    )
    .unwrap();
    drop(markers.start());
    let crate::MarkerAction::Done(GitResult::Failed { reason: run::DeliveryReason::TooLarge, .. }) =
        markers.from_file(skein_io::file::Event::TooLarge { owner, size: 65 })
    else {
        panic!("oversized file is not clean")
    };
}
