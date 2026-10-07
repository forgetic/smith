//! Two saturated searches, their four output routes and root authority
//! against the machine component's checked worst case.

use skein_lib::stream::Up;
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_world::domain::heap::{Counting, Meter};
use smith_domain::{run, tools};
use smith_protocol_machine::{self as machine, Below, BelowEvent, Component, FromDomain, Limits, ToDomain};

#[global_allocator]
static HEAP: Counting = Counting;

#[test]
fn saturated_searches_and_pipe_routes_stay_within_the_machine_bound() {
    let limits = Limits {
        operations: 0,
        roots: 1,
        path_bytes: 16,
        file_bytes: 0,
        entries: 0,
        entry_bytes: 0,
        processes: 2,
        output_bytes: 0,
        search_hits: 0,
        search_bytes: 256,
        search_line_bytes: 256,
        env_bytes: 0,
        stop_grace: Duration::from_millis(1),
    };
    let bound = machine::worst_case(&limits).expect("representable machine bound");
    let workspace = run::Workspace {
        directories: Box::new([run::Directory {
            name: Box::from(&b"repo"[..]),
            root: Token::new(1),
            writable: false,
            git: true,
            conflicts: Box::new([]),
        }]),
    };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut to_domain = Queue::<ToDomain>::with_capacity(4);
    let mut below = Queue::<Below>::with_capacity(8);
    let meter = Meter::new();
    meter.start();
    let mut component = Component::new(&limits);
    component.workspace(&workspace);
    meter.check(meter.end(), bound, &"new and workspace");

    for index in 0_u64..2 {
        let owner = Token::new(index + 10);
        let request = FromDomain::Op {
            owner,
            op: tools::Op::Search {
                at: tools::Place { root: Token::new(1), path: Box::new([]) },
                pattern: Box::from(&b"needle"[..]),
                glob: None,
                hits: 0,
                bytes: 256,
            },
            deadline: Time::from_nanos(100),
        };
        meter.start();
        component.from_domain(&env, request, &mut to_domain, &mut below);
        let measured = meter.end();
        match below.pop() {
            Some(Below::Spawn { .. }) => {}
            other => panic!("search starts a child: {other:?}"),
        }
        meter.check(measured, bound, &"search admitted");

        meter.start();
        component.from_below(
            &env,
            BelowEvent::Process(skein_io::Event::Spawned {
                owner,
                child: Token::new(index + 20),
                pipes: Box::new([Token::new(index * 2 + 30), Token::new(index * 2 + 31)]),
            }),
            &mut to_domain,
            &mut below,
        );
        let measured = meter.end();
        assert!(below.pop().is_some() && below.pop().is_some(), "both pipe demands");
        meter.check(measured, bound, &"both pipe routes");
    }
    assert!(to_domain.is_empty(), "both searches remain live");
    let line =
        br#"{"type":"match","data":{"path":{"text":"a"},"lines":{"text":"needle\n"},"line_number":1,"submatches":[]}}"#;
    meter.start();
    for byte in line.iter().copied().chain([b'\n']) {
        component.from_below(
            &env,
            BelowEvent::Process(skein_io::Event::Stream { owner: Token::new(30), up: Up::Bytes(Box::new([byte])) }),
            &mut to_domain,
            &mut below,
        );
        assert!(below.pop().is_some(), "stream rearms after each byte");
    }
    meter.check(meter.end(), bound, &"bounded JSON line parse");
    assert!(meter.held() > 0, "the component retains bounded state");
}
