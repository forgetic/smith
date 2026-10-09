//! Native trace observations remain complete across host output pressure and
//! every admitted terminal. The independent channel peer supplies the answer.
use smith_agent_process_world::{TraceCase, World, charter};
use smith_channel::RunResult;
use smith_domain::run::facts::{Answered, FactKind};

fn terminal(world: &World) -> Answered {
    let terminals: Vec<_> = world
        .facts()
        .iter()
        .filter_map(|fact| match fact {
            smith_domain::Fact::Run { fact } => match fact.kind {
                FactKind::Answered { answer, .. } => Some(answer),
                FactKind::MessageRead { .. }
                | FactKind::MessageFence { .. }
                | FactKind::MessageUnread { .. }
                | FactKind::MessageReceived { .. }
                | FactKind::MessageRefused { .. }
                | FactKind::Admitted { .. }
                | FactKind::Prepared { .. }
                | FactKind::Opened { .. }
                | FactKind::Ended { .. }
                | FactKind::Called { .. }
                | FactKind::Returned { .. }
                | FactKind::CheckStarted { .. }
                | FactKind::CheckFinished { .. }
                | FactKind::Delivered { .. } => None,
            },
            smith_domain::Fact::Session { .. } => None,
        })
        .collect();
    assert_eq!(terminals.len(), 1, "one native terminal reaches the trace, after the answer");
    assert_eq!(world.trace_loss(), 0, "trace room is independent of channel room");
    terminals[0]
}

#[test]
fn a_host_slow_to_acknowledge_loses_projected_facts_and_the_trace_keeps_them() {
    let mut narrow = World::new(7, &charter());
    narrow.observe_facts(TraceCase::Failed, 2);
    assert!(narrow.settle());
    let mut wide = World::new(7, &charter());
    wide.observe_facts(TraceCase::Failed, 8);
    assert!(wide.settle());
    assert_eq!(narrow.answer(), wide.answer(), "projection cannot change the run's answer");
    assert_eq!(terminal(&narrow), terminal(&wide));
    assert_eq!(narrow.facts(), wide.facts(), "each native emission survives the constrained output window");
    assert!(narrow.channel_loss() > 0);
    let projected = narrow.observed().iter().filter(|frame| frame.kind == 0x010f).count();
    let wide_projected = wide.observed().iter().filter(|frame| frame.kind == 0x010f).count();
    assert!(projected < wide_projected, "the constrained channel loses actual projected facts");
}

#[test]
fn every_answer_kind_reaches_the_trace_with_its_terminal_fact_once() {
    for case in [TraceCase::Accepted, TraceCase::Failed, TraceCase::Refused, TraceCase::Parked, TraceCase::Cancelled] {
        let mut world = World::new(8, &charter());
        world.observe_facts(case, 4);
        assert!(world.settle(), "{case:?}");
        let answer = world.answer().expect("one answer crossed the host channel");
        let ended = if case == TraceCase::Refused {
            assert!(world.facts().is_empty(), "an entrance refusal creates no native run or session");
            assert_eq!(world.trace_loss(), 0);
            // The service's actual answer is the refused terminal authority.
            // Its stream-local run.completed is introduced by increment02.5.
            None
        } else {
            Some(terminal(&world))
        };
        match case {
            TraceCase::Accepted => {
                assert!(matches!(answer.result(), RunResult::Accepted(_)));
                assert_eq!(ended, Some(Answered::Accepted));
            }
            TraceCase::Failed => {
                assert!(matches!(answer.result(), RunResult::Failed(_)));
                assert!(matches!(ended, Some(Answered::Failed(_))));
            }
            TraceCase::Refused => {
                assert!(matches!(answer.result(), RunResult::Refused(_)));
                assert_eq!(ended, None);
            }
            TraceCase::Parked => {
                assert!(matches!(answer.result(), RunResult::Parked));
                assert_eq!(ended, Some(Answered::Parked));
            }
            TraceCase::Cancelled => {
                assert!(matches!(answer.result(), RunResult::Failed(_)));
                assert_eq!(ended, Some(Answered::Failed(smith_domain::run::Failure::Cancelled)));
            }
        }
        world.assert_agent_clean();
    }
}
