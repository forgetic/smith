//! Constrained output and acknowledgement windows never remove trace facts.
use smith_agent_process_world::{TraceCase, World, charter};

fn vocabulary(world: &World) -> Vec<String> {
    world
        .facts()
        .iter()
        .map(|fact| match fact {
            smith_domain::Fact::Run { fact } => format!("run {:?}", fact.kind),
            smith_domain::Fact::Session { fact } => format!("session {:?}", fact.kind),
        })
        .collect()
}

#[test]
fn seeded_output_and_acknowledgement_windows_count_projection_loss_without_changing_the_trace() {
    for seed in 0..32_u64 {
        let mut roomy = World::new(seed, &charter());
        roomy.observe_facts(TraceCase::Failed, 8);
        assert!(roomy.settle());
        let mut constrained = World::new(seed, &charter());
        constrained.observe_facts(TraceCase::Failed, 2 + u32::try_from(seed % 7).expect("small window"));
        constrained.acknowledgement_window(1 + u32::try_from(seed % 8).expect("small window"));
        assert!(constrained.settle());
        assert_eq!(constrained.answer(), roomy.answer(), "seed {seed}");
        assert_eq!(vocabulary(&constrained), vocabulary(&roomy), "seed {seed}");
        assert_eq!(constrained.trace_loss(), 0, "seed {seed}");
        assert!(constrained.channel_loss() >= 1, "the post-answer terminal is counted, seed {seed}");
        constrained.assert_agent_clean();
    }
}
