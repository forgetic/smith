//! Mutated and truncated event lines never panic, for deterministic seeds.

#[path = "support/events.rs"]
mod fixtures;

use skein_lib::Rng;
use smith_events::{Limits, read, write};

#[test]
fn truncated_and_corrupted_event_lines_are_always_bounded_refusals_or_values() {
    let limits = Limits { string: 64, content: 1024, items: 8 };
    for (name, record, capture) in fixtures::samples() {
        let line = write(&record, &capture, &limits).expect(name).expect("record");
        for length in (0..line.len()).step_by(line.len().checked_div(16).expect("nonzero divisor").max(1)) {
            let truncated = line.get(..length).expect("prefix");
            let result = read(truncated, &limits);
            if length < line.len().saturating_sub(1) {
                assert!(result.is_err(), "truncated {name} at {length}");
            }
        }
        let mut rng = Rng::new(42);
        for _ in 0..16 {
            let mut changed = line.to_vec();
            let position = usize::try_from(rng.next_u64()).expect("machine word") % changed.len();
            *changed.get_mut(position).expect("position") = *rng.next_u64().to_le_bytes().first().expect("one byte");
            let result = read(&changed, &limits);
            drop(result);
        }
    }
}
