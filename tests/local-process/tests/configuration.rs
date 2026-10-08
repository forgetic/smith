//! The shared tool-domain ceilings must reach their machine adapter.
use skein_lib::{Duration, Env, Queue, Time, Token};
use smith_domain::{run, tools};
use smith_local_process_world::process;
use smith_protocol_machine::{Below, Component, FromDomain};

#[test]
fn declared_read_search_shell_and_check_bounds_reach_the_machine() {
    let config = process::lower_configuration();
    let tool_limits = config.limits.domain.session.tools;
    let root = Token::new(1);
    let place = || tools::Place { root, path: b"original.txt".as_slice().into() };
    let owner = Token::new(7);
    let deadline = Time::ZERO.saturating_add(Duration::from_secs(1));
    let requests = [
        FromDomain::Op { owner, op: tools::Op::Load { at: place(), max: tool_limits.file_bytes }, deadline },
        FromDomain::Op {
            owner,
            op: tools::Op::Search {
                at: place(),
                pattern: b"original".as_slice().into(),
                glob: None,
                hits: tool_limits.search_hits,
                bytes: tool_limits.search_bytes,
            },
            deadline,
        },
        FromDomain::Op {
            owner,
            op: tools::Op::Spawn {
                cwd: tools::Place { root, path: b".".as_slice().into() },
                command: b"printf ran".as_slice().into(),
                env: Box::new([]),
                roots: Box::new([]),
                head: tool_limits.shell_head,
                tail: tool_limits.shell_tail,
            },
            deadline,
        },
        FromDomain::Check {
            owner,
            program: run::Place { root, path: b".smith-test/check".as_slice().into() },
            deadline,
            tail: config.limits.domain.run.check_tail,
        },
    ];
    for request in requests {
        let mut machine = Component::new(&config.limits.machine);
        machine.workspace(&run::Workspace {
            directories: Box::new([run::Directory {
                name: b"repo".as_slice().into(),
                root,
                writable: true,
                git: true,
                conflicts: Box::new([]),
            }]),
        });
        let env = Env { now: Time::ZERO, wall: skein_tls_world::pki::VALID, limits: config.limits.machine };
        let mut up = Queue::with_capacity(16);
        let mut down = Queue::with_capacity(16);
        machine.from_domain(&env, request, &mut up, &mut down);
        assert!(up.is_empty(), "a request within the declared domain ceiling is admitted");
        assert!(
            matches!(down.pop(), Some(Below::File { .. } | Below::Spawn { .. })),
            "admitted domain work reaches the file or child boundary"
        );
        assert!(down.is_empty());
    }
}
