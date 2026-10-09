//! The machine's public stories (protocol/agent.md, section 8).

use skein_fake_machine::Item;
use skein_lib::{Time, Token};
use smith_domain::{run, tools};
use smith_machine_world::World;
use smith_protocol_machine::{FromDomain, ToDomain};

fn place(root: Token, path: &[u8]) -> tools::Place {
    tools::Place { root, path: Box::from(path) }
}

fn far() -> Time {
    Time::from_nanos(100_000_000)
}

#[test]
fn a_file_changed_behind_the_sessions_back_refuses_the_store() {
    let mut world = World::new(4, &[Item::file(b"a", b"old")]);
    let root = world.root();
    let loaded = world.request(FromDomain::Op {
        owner: Token::new(10),
        op: tools::Op::Load { at: place(root, b"a"), max: 10 },
        deadline: far(),
    });
    let version = match loaded {
        ToDomain::Done { done: tools::Done::Loaded { content, version }, .. } => {
            assert_eq!(content.as_ref(), b"old");
            version
        }
        other @ (ToDomain::Done { .. }
        | ToDomain::Read { .. }
        | ToDomain::Probed { .. }
        | ToDomain::Checked { .. }
        | ToDomain::Aborted { .. }) => panic!("expected load: {other:?}"),
    };
    world.change_behind_session(b"a", b"new");
    let done = world.request(FromDomain::Op {
        owner: Token::new(11),
        op: tools::Op::Store {
            at: place(root, b"a"),
            content: Box::from(&b"ours"[..]),
            expect: tools::Expect::Is { version },
        },
        deadline: far(),
    });
    assert!(matches!(done, ToDomain::Done { done: tools::Done::Conflict { .. }, .. }), "{done:?}");
    assert_eq!(world.content(b"a"), b"new");
    world.settle();
}

#[test]
fn a_path_with_a_git_part_or_through_a_symbolic_link_is_refused() {
    let mut world = World::new(5, &[Item::file(b"safe", b"old"), Item::link(b"link", b"safe")]);
    let root = world.root();
    let denied = world.request(FromDomain::Op {
        owner: Token::new(12),
        op: tools::Op::Store {
            at: place(root, b".GiT/config"),
            content: Box::from(&b"x"[..]),
            expect: tools::Expect::Absent,
        },
        deadline: far(),
    });
    assert_eq!(
        denied,
        ToDomain::Done { owner: Token::new(12), done: tools::Done::Failed { fault: tools::Fault::Denied } }
    );
    let linked = world.request(FromDomain::Op {
        owner: Token::new(13),
        op: tools::Op::Store { at: place(root, b"link"), content: Box::from(&b"x"[..]), expect: tools::Expect::Absent },
        deadline: far(),
    });
    assert_eq!(linked, ToDomain::Done { owner: Token::new(13), done: tools::Done::Linked });
    assert_eq!(world.content(b"safe"), b"old");
    world.settle();
}

#[test]
fn checks_that_pass_fail_and_time_out() {
    let mut world = World::new(6, &[]);
    for (number, name, expected) in [
        (20, &b"check-ok"[..], run::Exit::Code { code: 0 }),
        (21, &b"check-fail"[..], run::Exit::Code { code: 1 }),
        (22, &b"check-hang"[..], run::Exit::TimedOut),
    ] {
        let done = world.request(FromDomain::Check {
            owner: Token::new(number),
            program: run::Place { root: world.root(), path: Box::from(name) },
            deadline: Time::from_nanos(10_000_000),
            tail: 8,
        });
        assert_eq!(
            done,
            ToDomain::Checked {
                owner: Token::new(number),
                ran: run::Ran { exit: expected, output: Box::new([]), cut: 0 }
            }
        );
    }
    world.settle();
}

#[test]
fn a_command_that_hangs_is_stopped_at_its_deadline_and_answered_once_gone() {
    let mut world = World::new(7, &[]);
    let root = world.root();
    let done = world.request(FromDomain::Op {
        owner: Token::new(30),
        op: tools::Op::Spawn {
            cwd: place(root, b""),
            command: Box::from(&b"hang"[..]),
            env: Box::new([]),
            roots: Box::new([]),
            head: 4,
            tail: 4,
        },
        deadline: Time::from_nanos(1_000_000),
    });
    assert_eq!(
        done,
        ToDomain::Done {
            owner: Token::new(30),
            done: tools::Done::Exited {
                exit: tools::Exit::TimedOut,
                head: Box::new([]),
                tail: Box::new([]),
                dropped: 0,
            }
        }
    );
    world.settle();
}

#[test]
fn a_command_that_floods_its_output_keeps_its_head_and_tail() {
    let mut world = World::new(8, &[]);
    let output: Vec<u8> = (0_u32..4096).map(|at| b'a' + u8::try_from(at % 26).expect("alphabet")).collect();
    let done = world.flood(Token::new(40), &output, 8, 8);
    assert_eq!(
        done,
        ToDomain::Done {
            owner: Token::new(40),
            done: tools::Done::Exited {
                exit: tools::Exit::Code { code: 0 },
                head: Box::from(&output[..8]),
                tail: Box::from(&output[output.len() - 8..]),
                dropped: 4080,
            }
        }
    );
    world.settle();
}

#[test]
fn referee_writes_only_within_a_writable_root_and_answers_each_call_once() {
    let mut readonly = World::read_only(9, &[Item::file(b"kept", b"old")]);
    let denied = readonly.request(FromDomain::Op {
        owner: Token::new(41),
        op: tools::Op::Store {
            at: place(readonly.root(), b"kept"),
            content: Box::from(&b"new"[..]),
            expect: tools::Expect::Absent,
        },
        deadline: far(),
    });
    assert_eq!(
        denied,
        ToDomain::Done { owner: Token::new(41), done: tools::Done::Failed { fault: tools::Fault::Denied } }
    );
    assert_eq!(readonly.content(b"kept"), b"old");
    readonly.settle();

    let mut writable = World::new(10, &[Item::file(b"kept", b"old")]);
    let loaded = writable.request(FromDomain::Op {
        owner: Token::new(42),
        op: tools::Op::Load { at: place(writable.root(), b"kept"), max: 16 },
        deadline: far(),
    });
    let version = match loaded {
        ToDomain::Done { done: tools::Done::Loaded { version, .. }, .. } => version,
        other @ (ToDomain::Done { .. }
        | ToDomain::Read { .. }
        | ToDomain::Probed { .. }
        | ToDomain::Checked { .. }
        | ToDomain::Aborted { .. }) => panic!("expected a load: {other:?}"),
    };
    let stored = writable.request(FromDomain::Op {
        owner: Token::new(43),
        op: tools::Op::Store {
            at: place(writable.root(), b"kept"),
            content: Box::from(&b"new"[..]),
            expect: tools::Expect::Is { version },
        },
        deadline: far(),
    });
    assert!(matches!(stored, ToDomain::Done { owner, done: tools::Done::Stored { .. } } if owner == Token::new(43)));
    assert_eq!(writable.content(b"kept"), b"new");
    writable.settle();
}

#[test]
fn full_kernel_trace_and_file_terminal_replay_through_the_shared_kit() {
    let trace = skein_world::domain::assert_replays(40, 41, |seed| {
        let bytes = vec![b'x'; usize::try_from(seed % 16 + 1).expect("bounded seeded file")];
        let mut world = World::new(seed, &[Item::file(b"file", &bytes)]);
        let terminal = world.request(FromDomain::Op {
            owner: Token::new(40),
            op: tools::Op::Load { at: place(world.root(), b"file"), max: 16 },
            deadline: far(),
        });
        world.settle();
        (world.trace().to_vec(), terminal)
    });
    assert!(!trace.is_empty(), "the complete file IO trace was retained");
}

#[test]
fn successive_harness_runs_preserve_the_absolute_deadline_clock() {
    let mut world = World::new(42, &[]);
    let first = world.request(FromDomain::Check {
        owner: Token::new(50),
        program: run::Place { root: world.root(), path: b"check-hang".as_slice().into() },
        deadline: Time::from_nanos(10_000_000),
        tail: 8,
    });
    assert!(matches!(first, ToDomain::Checked { ran: run::Ran { exit: run::Exit::TimedOut, .. }, .. }));
    let before = world.now();
    assert!(before >= Time::from_nanos(10_000_000), "the first deadline elapsed");
    let second = world.request(FromDomain::Check {
        owner: Token::new(51),
        program: run::Place { root: world.root(), path: b"check-hang".as_slice().into() },
        deadline: Time::from_nanos(1_000_000),
        tail: 8,
    });
    assert!(matches!(second, ToDomain::Checked { ran: run::Ran { exit: run::Exit::TimedOut, .. }, .. }));
    assert_eq!(world.now(), before, "an expired absolute deadline does not restart its duration");
    world.settle();
}

#[test]
fn listing_and_searching_the_directory_itself_reach_io_as_dot() {
    let mut world = World::new(11, &[Item::file(b"a", b"x")]);
    let root = world.root();
    let listed = world.request(FromDomain::Op {
        owner: Token::new(50),
        op: tools::Op::Scan { at: place(root, b""), max: 16, max_bytes: 2048 },
        deadline: far(),
    });
    assert!(matches!(listed, ToDomain::Done { done: tools::Done::Scanned { ref entries, more: 0 }, .. }
        if entries.len() == 1 && entries[0].name.as_bytes() == b"a"));
    let searched = world.request(FromDomain::Op {
        owner: Token::new(51),
        op: tools::Op::Search { at: place(root, b""), pattern: Box::from(&b"x"[..]), glob: None, hits: 8, bytes: 128 },
        deadline: far(),
    });
    assert!(matches!(searched, ToDomain::Done { done: tools::Done::Found { .. }, .. }), "{searched:?}");
    world.settle();
}
