//! Deliberately bad boundary histories must fail the local-world referee.

use skein_lib::{Time, Token};
use skein_world::domain::{Referee, Verdict};
use smith_local_world::referee::{Meeting, Seen};

fn observe(referee: &mut Referee<Meeting>, seen: Seen) {
    referee.observe(Time::ZERO, seen, &mut Vec::new());
}

fn broken(mut history: Vec<Seen>, reason: &str) {
    let mut referee = Referee::new(Meeting::default());
    for seen in history.drain(..) {
        observe(&mut referee, seen);
    }
    let Verdict::Failed(failure) = referee.verdict() else { panic!("bad history must fail") };
    assert_eq!(failure.why, reason);
}

#[test]
fn a_reused_activation_is_rejected() {
    broken(vec![Seen::Activation { number: 0, message: 1 }], "activations strictly increase across invocations");
}

#[test]
fn an_out_of_order_turn_save_is_rejected() {
    broken(
        vec![
            Seen::Activation { number: 1, message: 1 },
            Seen::Turn { number: 1 },
            Seen::SaveTurn { number: 1, read: None },
            Seen::Turn { number: 2 },
            Seen::SaveTurn { number: 2, read: None },
            Seen::TurnSaved { number: 2 },
        ],
        "turns become durable in order",
    );
}

#[test]
fn an_answer_shown_before_its_turn_is_durable_is_rejected() {
    broken(
        vec![
            Seen::Activation { number: 1, message: 1 },
            Seen::Turn { number: 1 },
            Seen::SaveTurn { number: 1, read: None },
            Seen::Answered { activation: 1 },
            Seen::Shown { activation: 1 },
        ],
        "the answer is shown after all turns are saved",
    );
}

#[test]
fn a_read_fence_without_a_relayed_line_is_rejected() {
    broken(
        vec![
            Seen::Activation { number: 1, message: 1 },
            Seen::Turn { number: 1 },
            Seen::SaveTurn { number: 1, read: Some(Token::new(1)) },
        ],
        "a read fence names a relayed person line",
    );
}

#[test]
fn a_duplicate_provider_terminal_is_rejected() {
    broken(
        vec![
            Seen::Complete { owner: Token::new(2) },
            Seen::Completed { owner: Token::new(2) },
            Seen::Completed { owner: Token::new(2) },
        ],
        "one terminal per completion request",
    );
}

#[test]
fn a_duplicate_answer_is_rejected() {
    broken(
        vec![
            Seen::Activation { number: 1, message: 1 },
            Seen::Answered { activation: 1 },
            Seen::Answered { activation: 1 },
        ],
        "one answer per run",
    );
}

#[test]
fn a_turn_saved_after_the_result_was_shown_is_rejected() {
    broken(
        vec![
            Seen::Activation { number: 1, message: 1 },
            Seen::Answered { activation: 1 },
            Seen::Shown { activation: 1 },
            Seen::Turn { number: 1 },
        ],
        "nothing is told after the shown answer",
    );
}
