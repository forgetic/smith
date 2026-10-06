use smith_local_world::{Cut, World};

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
