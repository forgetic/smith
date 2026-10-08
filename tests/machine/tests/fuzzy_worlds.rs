//! Seeded file changes, failed children and deadlines over actual simulated IO.

use skein_fake_machine::Item;
use skein_lib::{Rng, Time, Token};
use smith_domain::{run, tools};
use smith_machine_world::World;
use smith_protocol_machine::{FromDomain, ToDomain};

fn place(root: Token, path: &[u8]) -> tools::Place {
    tools::Place { root, path: Box::from(path) }
}

fn load(world: &mut World, owner: Token) -> tools::Version {
    let done = world.request(FromDomain::Op {
        owner,
        op: tools::Op::Load { at: place(world.root(), b"file"), max: 16 },
        deadline: Time::from_nanos(100_000_000),
    });
    match done {
        ToDomain::Done { done: tools::Done::Loaded { version, .. }, .. } => version,
        other @ (ToDomain::Done { .. }
        | ToDomain::Read { .. }
        | ToDomain::Probed { .. }
        | ToDomain::Checked { .. }
        | ToDomain::Aborted { .. }) => panic!("seeded load: {other:?}"),
    }
}

fn run(seed: u64) -> (usize, ToDomain, Vec<String>) {
    let mut rng = Rng::new(seed);
    let mode = usize::try_from(rng.below(7)).expect("mode fits");
    let mut world = if mode == 6 {
        World::read_only(seed, &[Item::file(b"file", b"old")])
    } else {
        World::new(seed, &[Item::file(b"file", b"old")])
    };
    let root = world.root();
    let owner = Token::new(100);
    let deadline = Time::from_nanos(10_000_000);
    let terminal = match mode {
        0 | 1 | 6 => {
            let version = load(&mut world, Token::new(99));
            if mode == 0 {
                world.change_behind_session(b"file", b"new");
            }
            let done = world.request(FromDomain::Op {
                owner,
                op: tools::Op::Store {
                    at: place(root, b"file"),
                    content: Box::from(&b"ours"[..]),
                    expect: tools::Expect::Is { version },
                },
                deadline,
            });
            match mode {
                0 => assert!(
                    matches!(done, ToDomain::Done { done: tools::Done::Conflict { .. }, .. }),
                    "seed {seed}: {done:?}"
                ),
                1 => assert!(
                    matches!(done, ToDomain::Done { done: tools::Done::Stored { .. }, .. }),
                    "seed {seed}: {done:?}"
                ),
                6 => assert_eq!(
                    done,
                    ToDomain::Done { owner, done: tools::Done::Failed { fault: tools::Fault::Denied } }
                ),
                2..=5 | 7.. => unreachable!("store modes"),
            }
            done
        }
        2 | 3 => {
            let command = if mode == 2 { b"fail".as_slice() } else { b"hang".as_slice() };
            let done = world.request(FromDomain::Op {
                owner,
                op: tools::Op::Spawn {
                    cwd: place(root, b""),
                    command: Box::from(command),
                    env: Box::new([]),
                    roots: Box::new([]),
                    head: 8,
                    tail: 8,
                },
                deadline,
            });
            let exit = if mode == 2 { tools::Exit::Code { code: 1 } } else { tools::Exit::TimedOut };
            assert_eq!(
                done,
                ToDomain::Done {
                    owner,
                    done: tools::Done::Exited { exit, head: Box::new([]), tail: Box::new([]), dropped: 0 }
                },
                "seed {seed}"
            );
            done
        }
        4 | 5 => {
            let program = if mode == 4 { b"check-fail".as_slice() } else { b"check-hang".as_slice() };
            let done = world.request(FromDomain::Check {
                owner,
                program: run::Place { root, path: Box::from(program) },
                deadline,
                tail: 8,
            });
            let exit = if mode == 4 { run::Exit::Code { code: 1 } } else { run::Exit::TimedOut };
            assert_eq!(
                done,
                ToDomain::Checked { owner, ran: run::Ran { exit, output: Box::new([]), cut: 0 } },
                "seed {seed}"
            );
            done
        }
        7.. => unreachable!("seven modes"),
    };
    world.settle();
    (mode, terminal, world.trace().to_vec())
}

#[test]
fn seeded_file_changes_child_failures_and_deadlines_replay_and_settle() {
    let mut covered = [0_u32; 7];
    for seed in 100_u64..228 {
        let first = run(seed);
        let second = run(seed);
        assert_eq!(first, second, "seed {seed} replays its terminal and complete kernel trace");
        let slot = covered.get_mut(first.0).expect("one of seven modes");
        *slot = slot.checked_add(1).expect("seed count fits");
    }
    assert!(covered.iter().all(|count| *count > 0), "every fault class appeared: {covered:?}");
}
