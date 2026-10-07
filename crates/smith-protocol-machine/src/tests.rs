//! Entry-point tests for file admission and translation.

use alloc::boxed::Box;
use skein_io::{digest, file, kernel};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use smith_domain::{run, tools};

use crate::{Below, BelowEvent, Component, FromDomain, Limits, ToDomain};

fn limits() -> Limits {
    Limits {
        operations: 2,
        roots: 1,
        path_bytes: 64,
        file_bytes: 16,
        entries: 2,
        entry_bytes: 512,
        processes: 2,
        output_bytes: 64,
        search_hits: 8,
        search_bytes: 256,
        search_line_bytes: 512,
        env_bytes: 512,
        stop_grace: Duration::from_millis(10),
    }
}

fn env() -> Env<Limits> {
    Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits() }
}

fn component() -> Component {
    let mut component = Component::new(&limits());
    component.workspace(&run::Workspace {
        directories: Box::new([run::Directory {
            name: Box::from(&b"code"[..]),
            root: Token::new(10),
            writable: true,
            git: true,
            conflicts: Box::new([]),
        }]),
    });
    component
}

fn place(path: &[u8]) -> tools::Place {
    tools::Place { root: Token::new(10), path: Box::from(path) }
}

#[test]
fn git_part_and_unknown_root_are_refused_before_io() {
    let mut component = component();
    let mut to_domain = Queue::with_capacity(2);
    let mut below = Queue::with_capacity(2);
    component.from_domain(
        &env(),
        FromDomain::Op {
            owner: Token::new(1),
            op: tools::Op::Load { at: place(b"src/.GIT/config"), max: 16 },
            deadline: Time::from_nanos(u64::MAX),
        },
        &mut to_domain,
        &mut below,
    );
    assert!(below.is_empty());
    assert_eq!(
        to_domain.pop(),
        Some(ToDomain::Done { owner: Token::new(1), done: tools::Done::Failed { fault: tools::Fault::Denied } })
    );
    component.from_domain(
        &env(),
        FromDomain::Op {
            owner: Token::new(2),
            op: tools::Op::Load { at: tools::Place { root: Token::new(99), path: Box::from(&b"a"[..]) }, max: 16 },
            deadline: Time::from_nanos(u64::MAX),
        },
        &mut to_domain,
        &mut below,
    );
    assert!(below.is_empty());
    assert_eq!(to_domain.pop(), Some(ToDomain::Done { owner: Token::new(2), done: tools::Done::Escapes }));
}

#[test]
fn store_uses_no_follow_and_returns_digest() {
    let mut component = component();
    let mut to_domain = Queue::with_capacity(2);
    let mut below = Queue::with_capacity(2);
    component.from_domain(
        &env(),
        FromDomain::Op {
            owner: Token::new(1),
            op: tools::Op::Store {
                at: place(b"src/a"),
                content: Box::from(&b"new"[..]),
                expect: tools::Expect::Absent,
            },
            deadline: Time::from_nanos(u64::MAX),
        },
        &mut to_domain,
        &mut below,
    );
    assert_eq!(
        below.pop(),
        Some(Below::File {
            request: file::Request::Store {
                owner: Token::new(1),
                root: Token::new(10),
                path: Box::from(&b"src/a"[..]),
                bytes: Box::from(&b"new"[..]),
                expected: None,
                no_follow: true,
            },
            deadline: Time::from_nanos(u64::MAX)
        })
    );
    let version = digest::digest(b"new");
    component.from_below(
        &env(),
        BelowEvent::File(file::Event::Stored { owner: Token::new(1), digest: version }),
        &mut to_domain,
        &mut below,
    );
    assert_eq!(
        to_domain.pop(),
        Some(ToDomain::Done {
            owner: Token::new(1),
            done: tools::Done::Stored { version: tools::Version::new(version.0) }
        })
    );
}

#[test]
fn a_symbolic_link_refusal_becomes_linked_and_a_scan_keeps_more() {
    let mut component = component();
    let mut to_domain = Queue::with_capacity(2);
    let mut below = Queue::with_capacity(2);
    component.from_domain(
        &env(),
        FromDomain::Op {
            owner: Token::new(1),
            op: tools::Op::Store { at: place(b"link/a"), content: Box::from(&b"x"[..]), expect: tools::Expect::Absent },
            deadline: Time::from_nanos(u64::MAX),
        },
        &mut to_domain,
        &mut below,
    );
    assert!(below.pop().is_some());
    component.from_below(
        &env(),
        BelowEvent::File(file::Event::Failed { owner: Token::new(1), error: kernel::Error::TooManyLinks }),
        &mut to_domain,
        &mut below,
    );
    assert_eq!(to_domain.pop(), Some(ToDomain::Done { owner: Token::new(1), done: tools::Done::Linked }));
    component.from_domain(
        &env(),
        FromDomain::Op {
            owner: Token::new(2),
            op: tools::Op::Scan { at: place(b"src"), max: 2, max_bytes: 512 },
            deadline: Time::from_nanos(u64::MAX),
        },
        &mut to_domain,
        &mut below,
    );
    assert_eq!(
        below.pop(),
        Some(Below::File {
            request: file::Request::Scan {
                owner: Token::new(2),
                root: Token::new(10),
                path: Box::from(&b"src"[..]),
                max: 2,
                max_bytes: 512,
                no_follow: false,
            },
            deadline: Time::from_nanos(u64::MAX)
        })
    );
    component.from_below(
        &env(),
        BelowEvent::File(file::Event::Scanned {
            owner: Token::new(2),
            entries: Box::new([file::Entry { name: Box::from(&b"a"[..]), kind: kernel::Kind::File }]),
            more: 3,
        }),
        &mut to_domain,
        &mut below,
    );
    assert_eq!(
        to_domain.pop(),
        Some(ToDomain::Done {
            owner: Token::new(2),
            done: tools::Done::Scanned {
                entries: Box::new([tools::Entry {
                    name: tools::Name::new(Box::from(&b"a"[..])).expect("safe name"),
                    kind: tools::Kind::File
                }]),
                more: 3,
            }
        })
    );
}

#[test]
fn cancellation_is_sent_below_and_the_actual_terminal_decides_the_race() {
    let mut component = component();
    let mut to_domain = Queue::with_capacity(2);
    let mut below = Queue::with_capacity(2);
    component.from_domain(
        &env(),
        FromDomain::Op {
            owner: Token::new(1),
            op: tools::Op::Load { at: place(b"a"), max: 16 },
            deadline: Time::from_nanos(100),
        },
        &mut to_domain,
        &mut below,
    );
    assert_eq!(
        below.pop(),
        Some(Below::File {
            request: file::Request::Load {
                owner: Token::new(1),
                root: Token::new(10),
                path: Box::from(&b"a"[..]),
                max: 16,
                no_follow: false,
            },
            deadline: Time::from_nanos(100),
        })
    );
    component.from_domain(&env(), FromDomain::Cancel { owner: Token::new(1) }, &mut to_domain, &mut below);
    assert_eq!(below.pop(), Some(Below::CancelFile { owner: Token::new(1) }));
    component.from_below(
        &env(),
        BelowEvent::File(file::Event::Loaded { owner: Token::new(1), bytes: Box::from(&b"old"[..]) }),
        &mut to_domain,
        &mut below,
    );
    assert_eq!(
        to_domain.pop(),
        Some(ToDomain::Done {
            owner: Token::new(1),
            done: tools::Done::Loaded {
                content: Box::from(&b"old"[..]),
                version: tools::Version::new(digest::digest(b"old").0),
            }
        })
    );

    component.from_domain(
        &env(),
        FromDomain::Op {
            owner: Token::new(2),
            op: tools::Op::Load { at: place(b"b"), max: 16 },
            deadline: Time::from_nanos(100),
        },
        &mut to_domain,
        &mut below,
    );
    assert!(below.pop().is_some());
    component.from_domain(&env(), FromDomain::Cancel { owner: Token::new(2) }, &mut to_domain, &mut below);
    assert_eq!(below.pop(), Some(Below::CancelFile { owner: Token::new(2) }));
    component.from_below(
        &env(),
        BelowEvent::File(file::Event::Cancelled { owner: Token::new(2) }),
        &mut to_domain,
        &mut below,
    );
    assert_eq!(to_domain.pop(), Some(ToDomain::Done { owner: Token::new(2), done: tools::Done::Cancelled }));
}

#[test]
fn guide_read_cuts_utf8_and_closes_before_answer() {
    let mut component = component();
    let mut to_domain = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(3);
    let owner = Token::new(31);
    let deadline = Time::from_nanos(100);
    component.from_domain(
        &env(),
        FromDomain::Read {
            owner,
            at: run::Place { root: Token::new(10), path: Box::from(&b"AGENTS.md"[..]) },
            max: 3,
            deadline,
        },
        &mut to_domain,
        &mut below,
    );
    match below.pop() {
        Some(Below::OpenRead { .. }) => {}
        other => panic!("expected open read: {other:?}"),
    }
    let file = Token::new(41);
    component.from_below(
        &env(),
        BelowEvent::File(file::Event::Opened { owner, file, len: 4 }),
        &mut to_domain,
        &mut below,
    );
    assert_eq!(
        below.pop(),
        Some(Below::File { request: file::Request::ReadAt { owner, file, offset: 0, max: 3 }, deadline })
    );
    component.from_below(
        &env(),
        BelowEvent::File(file::Event::Read { owner, bytes: Box::from(&b"a\xc3\xa9"[..]) }),
        &mut to_domain,
        &mut below,
    );
    match below.pop() {
        Some(Below::File { request: file::Request::Close { .. }, .. }) => {}
        other => panic!("expected close: {other:?}"),
    }
    assert!(to_domain.is_empty());
    component.from_below(&env(), BelowEvent::File(file::Event::Closed { owner }), &mut to_domain, &mut below);
    assert_eq!(
        to_domain.pop(),
        Some(ToDomain::Read { owner, read: run::Read::Text { text: Box::from(&b"a\xc3\xa9"[..]), whole: false } })
    );
}

#[test]
fn command_output_waits_for_both_pipes_and_child_close() {
    use skein_io::Event;
    use skein_lib::stream::Up;
    let mut component = component();
    let mut to_domain = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(3);
    let owner = Token::new(32);
    component.from_domain(
        &env(),
        FromDomain::Op {
            owner,
            op: tools::Op::Spawn {
                cwd: place(b"."),
                command: Box::from(&b"printf abcdef"[..]),
                env: Box::new([]),
                roots: Box::new([]),
                head: 2,
                tail: 2,
            },
            deadline: Time::from_nanos(100),
        },
        &mut to_domain,
        &mut below,
    );
    match below.pop() {
        Some(Below::Spawn { owner: spawned, .. }) => assert_eq!(spawned, owner),
        other => panic!("expected spawn: {other:?}"),
    }
    component.from_below(
        &env(),
        BelowEvent::Process(Event::Spawned {
            owner,
            child: Token::new(42),
            pipes: Box::new([Token::new(43), Token::new(44)]),
        }),
        &mut to_domain,
        &mut below,
    );
    assert_eq!(below.len(), 2);
    assert!(below.pop().is_some());
    assert!(below.pop().is_some());
    component.from_below(
        &env(),
        BelowEvent::Process(Event::Stream { owner: Token::new(43), up: Up::Bytes(Box::from(&b"abcdef"[..])) }),
        &mut to_domain,
        &mut below,
    );
    assert!(below.pop().is_some());
    component.from_below(
        &env(),
        BelowEvent::Process(Event::Exited { owner, exit: kernel::Exit::Code(0) }),
        &mut to_domain,
        &mut below,
    );
    for pipe in [Token::new(43), Token::new(44)] {
        component.from_below(&env(), BelowEvent::Process(Event::Closed { owner: pipe }), &mut to_domain, &mut below);
    }
    assert!(to_domain.is_empty());
    component.from_below(&env(), BelowEvent::Process(Event::Closed { owner }), &mut to_domain, &mut below);
    assert_eq!(
        to_domain.pop(),
        Some(ToDomain::Done {
            owner,
            done: tools::Done::Exited {
                exit: tools::Exit::Code { code: 0 },
                head: Box::from(&b"ab"[..]),
                tail: Box::from(&b"ef"[..]),
                dropped: 2,
            }
        })
    );
}

#[test]
fn deadline_sends_term_then_kill_and_answers_after_reap() {
    use skein_io::Event;
    let mut component = component();
    let mut to_domain = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(3);
    let owner = Token::new(33);
    component.from_domain(
        &env(),
        FromDomain::Check {
            owner,
            program: run::Place { root: Token::new(10), path: Box::from(&b"check"[..]) },
            deadline: Time::from_nanos(100),
            tail: 4,
        },
        &mut to_domain,
        &mut below,
    );
    match below.pop() {
        Some(Below::Spawn { .. }) => {}
        other => panic!("expected spawn: {other:?}"),
    }
    component.from_below(
        &env(),
        BelowEvent::Process(Event::Spawned {
            owner,
            child: Token::new(45),
            pipes: Box::new([Token::new(46), Token::new(47)]),
        }),
        &mut to_domain,
        &mut below,
    );
    assert_eq!(below.len(), 2);
    assert!(below.pop().is_some());
    assert!(below.pop().is_some());
    let mut later = env();
    later.now = Time::from_nanos(101);
    component.fire(&later, &mut to_domain, &mut below);
    assert_eq!(
        below.pop(),
        Some(Below::Process(skein_io::Request::Signal { child: Token::new(45), signal: kernel::Signal::Terminate }))
    );
    later.now = Time::from_nanos(10_000_102);
    component.fire(&later, &mut to_domain, &mut below);
    assert_eq!(
        below.pop(),
        Some(Below::Process(skein_io::Request::Signal { child: Token::new(45), signal: kernel::Signal::Kill }))
    );
    component.from_below(
        &later,
        BelowEvent::Process(Event::Exited { owner, exit: kernel::Exit::Signal(9) }),
        &mut to_domain,
        &mut below,
    );
    for pipe in [Token::new(46), Token::new(47)] {
        component.from_below(&later, BelowEvent::Process(Event::Closed { owner: pipe }), &mut to_domain, &mut below);
    }
    assert!(to_domain.is_empty());
    component.from_below(&later, BelowEvent::Process(Event::Closed { owner }), &mut to_domain, &mut below);
    assert_eq!(
        to_domain.pop(),
        Some(ToDomain::Checked { owner, ran: run::Ran { exit: run::Exit::TimedOut, output: Box::new([]), cut: 0 } })
    );
}
