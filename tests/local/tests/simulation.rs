use skein_world::domain::assert_replays;
use smith_local_domain::ExitStatus;
use smith_local_world::{Cut, StoreFault, World};

#[test]
fn a_question_is_answered_and_the_chat_waits_for_the_next() {
    let mut world = World::new(17);
    world.line(b"What can you do?");
    assert!(world.drive(200), "the chat reaches a saved turn and a wait");
    assert_eq!(world.saved_turns(), 3, "the reply, yield and wait turns are durable in order");
    assert!(world.waiting(), "the run asked to wait for another person message");
    assert!(world.completions() >= 2, "the fake provider served the reply and wait");
    assert!(world.shown().iter().any(|text| text.as_ref() == b"Hello from the agent."));
}

#[test]
fn a_chat_with_no_workspace_waits_parks_and_resumes_on_the_next_invocation() {
    let mut first = World::new(21);
    first.line(b"Hello");
    assert!(first.drive_to_park(300), "first activation parks after waiting");
    assert_eq!(first.activation(), 1);
    let stored = first.into_store();
    let prior_turns = stored.turns();
    assert!(prior_turns > 0, "the parked chat kept its turns");

    let mut second = World::with_store(22, stored);
    second.line(b"I am back");
    assert!(second.drive(300), "a fresh domain resumes from {prior_turns} stored turns");
    assert_eq!(second.activation(), 2, "the next activation is durable before its agent runs");
    assert_eq!(second.prompt_assistants().first(), Some(&3), "provider sees the saved assistant history");
    assert!(second.shown().iter().any(|text| text.as_ref() == b"Welcome back."));
}

#[test]
fn a_crash_after_saving_an_activation_never_reuses_it() {
    let mut first = World::new(23);
    first.line(b"First attempt");
    assert!(first.drive_to_cut(100, Cut::AfterSaveState));
    let stored = first.into_store();
    assert_eq!(stored.activation(), 1);
    assert_eq!(stored.turns(), 0, "the child was never started");

    let mut second = World::with_store(24, stored);
    second.line(b"Retry");
    assert!(second.drive_to_cut(100, Cut::AfterSaveState));
    assert_eq!(second.activation(), 2, "the crashed activation is never reused");
}

#[test]
fn a_crash_before_a_turn_is_saved_resumes_from_the_turn_before_with_a_new_activation() {
    let mut first = World::new(25);
    first.line(b"Hello");
    assert!(first.drive_to_cut(300, Cut::BeforeSaveTurn(2)));
    let stored = first.into_store();
    assert_eq!(stored.turns(), 1, "the second turn did not reach durable storage");
    assert!(stored.read().is_none(), "the first turn did not yet read the person's line");

    let mut second = World::with_store(26, stored);
    second.line(b"Continue");
    assert!(second.drive(300), "the next activation resumes the saved prefix");
    assert_eq!(second.activation(), 2, "the replacement activation has a new name");
    assert_eq!(second.prompt_assistants().first(), Some(&1), "only the turn before the cut is replayable");
}

#[test]
fn a_crash_after_turn_saved_replays_that_turn() {
    let mut first = World::new(27);
    first.line(b"Hello");
    assert!(first.drive_to_cut(300, Cut::AfterTurnSaved(2)));
    let stored = first.into_store();
    assert_eq!(stored.turns(), 2);
    assert!(stored.read().is_some(), "the acknowledged turn's read fence is durable");
    let mut second = World::with_store(28, stored);
    second.line(b"Continue");
    assert!(second.drive(300), "the acknowledged turn is replayed");
    assert_eq!(second.activation(), 2);
    assert_eq!(second.prompt_assistants().first(), Some(&2), "the acknowledged turns are in the next prompt");
}

#[test]
fn a_transcript_the_agent_refuses_is_reported_and_nothing_starts() {
    let mut first = World::new(29);
    first.line(b"Hello");
    assert!(first.drive_to_park(300));
    let mut stored = first.into_store();
    stored.refuse_version();
    let mut second = World::with_store(30, stored);
    second.line(b"Continue");
    assert!(!second.drive(300), "a refused transcript cannot reach a wait");
    assert_eq!(second.completions(), 0, "refusal precedes provider effects");
    assert!(second.shown().iter().any(|text| text.as_ref() == b"Saved transcript was refused"));

    let stored = second.into_store();
    let mut fresh = World::with_store_fresh(31, stored);
    fresh.line(b"Start a fresh chat");
    assert!(fresh.drive(300), "resume false ignores the refused history");
    assert!(fresh.completions() > 0, "the fresh run reaches the provider");

    let stored = fresh.into_store();
    let mut resumed = World::with_store(32, stored);
    resumed.line(b"Continue after a fresh start");
    assert!(resumed.drive(300), "the replacement history is resumable");
}

#[test]
fn an_expired_credential_is_refreshed_and_the_completion_retried() {
    let mut world = World::new(41);
    world.reject_first_credential();
    world.line(b"Hello");
    assert!(world.drive(300), "the provider retry reaches the next wait");
    assert_eq!(world.credential_requests(), 2, "the rejected generation triggers one refresh");
    assert!(world.completions() >= 3, "the rejected completion was retried");
    assert!(world.shown().iter().any(|text| text.as_ref() == b"Hello from the agent."));
}

#[test]
fn an_exhausted_account_is_shown_to_the_person() {
    let mut world = World::new(44);
    world.exhaust_first_account();
    world.line(b"Hello");
    world.drive(300);
    assert!(world.shown().iter().any(|text| text.as_ref() == b"A model account is exhausted"));
}

#[test]
fn a_run_cancelled_at_the_terminal_answers_cancelled_after_its_last_turn_is_saved() {
    let mut world = World::new(42);
    world.slow_store();
    world.line(b"Hello");
    assert!(!world.drive(300), "the slow store holds a turn acknowledgement");
    assert!(world.delayed_turns() > 0);
    world.interrupt();
    assert!(!world.drive_to_cancelled(300), "the cancelled answer waits for the store");
    assert!(!world.shown().iter().any(|text| text.as_ref() == b"Run cancelled"));
    for _ in 0..4 {
        assert!(world.release_turn(), "a stalled turn must be acknowledged");
        if world.drive_to_cancelled(300) {
            break;
        }
    }
    assert!(
        world.shown().iter().any(|text| text.as_ref() == b"Run cancelled"),
        "the answer follows the last saved turn"
    );
    assert!(world.saved_turns() > 0);
}

#[test]
fn a_slow_store_pauses_the_agent_and_loses_no_turn() {
    let mut world = World::tight_unsaved(43);
    world.slow_store();
    world.line(b"Hello");
    assert!(!world.drive(300), "the full unsaved window pauses the chat");
    assert_eq!(world.saved_turns(), 1, "the second completion cannot tell another turn yet");
    assert_eq!(world.delayed_turns(), 1);

    for _ in 0..8 {
        if world.waiting() {
            break;
        }
        assert!(world.release_turn(), "each stalled turn has an acknowledgement to release");
        world.drive(300);
    }
    assert!(world.waiting(), "the paused agent resumes after store acknowledgements");
    assert_eq!(world.saved_turns(), 3, "all three conversation turns reached the store");
}

#[test]
fn a_chat_replays_the_same_boundary_trace_from_its_seed() {
    let trace = assert_replays(71, 72, |seed| {
        let mut world = World::new(seed);
        world.line(b"Hello");
        assert!(world.drive(300));
        (world.trace().to_vec(), (world.saved_turns(), world.shown().to_vec()))
    });
    assert!(trace.iter().any(|line| line.contains("TurnSaved")));
}

#[test]
fn facts_do_not_change_the_chat_when_their_queue_is_full() {
    let mut observed = World::new(73);
    observed.line(b"Hello");
    assert!(observed.drive(300));
    let mut dropped = World::without_facts(73);
    dropped.line(b"Hello");
    assert!(dropped.drive(300));
    assert_eq!(observed.saved_turns(), dropped.saved_turns());
    assert_eq!(observed.shown(), dropped.shown());
    assert!(dropped.facts_lost() > 0);
    assert!(observed.judged().0 > 0);
}

#[test]
fn a_store_failure_stops_the_chat_and_tells_the_person() {
    for fault in [StoreFault::Load, StoreFault::State, StoreFault::Turn] {
        let mut world = World::new(75);
        world.fail_store(fault);
        world.line(b"Hello");
        world.drive(300);
        assert_eq!(world.exit(), Some(ExitStatus::Failed), "{fault:?} failure exits");
        assert!(world.shown().iter().any(|text| text.as_ref() == b"The chat could not be saved"));
    }
}
