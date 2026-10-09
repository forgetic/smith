//! The spawned process adapter and its owned channel stay within their
//! checked bound (protocol/hosts.md, section 3; programming-model.md,
//! section 6.3).

#[cfg(test)]
mod tests {
    use skein_io::{self as io, kernel};
    use skein_lib::{Queue, Time, Token};
    use skein_world::domain::heap::{Counting, Meter};
    use smith_agent_process_world as agent_fixture;
    use smith_host_protocol as host;

    #[global_allocator]
    static HEAP: Counting = Counting;

    #[test]
    fn spawned_process_and_its_channel_fit_the_checked_memory_bound() {
        let agent = agent_fixture::limits();
        let limits = host::Limits {
            bodies: agent.channel.bodies,
            charter: agent.channel.charter,
            transcript: agent.channel.transcript,
            endpoints: agent.channel.endpoints,
            channel: agent.channel.channel,
            calls: 8,
        };
        let bound = host::process_worst_case(&limits, 32)
            .expect("checked process bound")
            .checked_add(Queue::<io::Request>::worst_case(8).expect("bounded output queue"))
            .expect("combined process bound");
        let meter = Meter::new();
        meter.start();
        let mut process =
            host::Process::new(Token::new(1), &limits, 32, agent_fixture::configuration().channel_endpoints)
                .expect("bounded process");
        let mut below = Queue::with_capacity(8);
        process.spawn(
            host::Launch {
                program: Box::from(&b"smith"[..]),
                arguments: Box::from([Box::from(&b"agent"[..])]),
                environment: Box::new([]),
                root: kernel::Fd::new(3),
                directory: Box::from(&b"."[..]),
            },
            Time::from_nanos(1_000_000),
            &mut below,
        );
        let peak = meter.check(meter.end(), bound, &"spawned process with opening request");
        assert!(peak > 0, "the counting allocator saw process state");
        drop((process, below));
    }

    #[test]
    fn hosted_parent_agent_and_peer_are_metered_at_every_iteration_and_drop() {
        use smith_hosts_world::{Program, World};
        for program in [Program::Service, Program::ErrorTail] {
            let mut world = World::new(7, program);
            world.check_memory();
            world.settle();
            assert!(world.seen().gone.is_some(), "the parent releases its slot");
            drop(world);
        }
    }
}
