//! Entry-point tests for file admission and translation.

use alloc::boxed::Box;
use skein_io::{digest, file, kernel};
use skein_lib::{Env, Queue, Time, Token, Wall};
use smith_domain::{run, tools};

use crate::{Below, BelowEvent, Component, FromDomain, Limits, ToDomain};

fn limits() -> Limits {
    Limits { operations: 2, roots: 1, path_bytes: 64, file_bytes: 16, entries: 2, entry_bytes: 512 }
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
