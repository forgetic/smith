//! Focused consumer stories for the shared kit
//! (testing-strategy.md, sections 2.2 and 6; domain/README.md, section 4).

mod common;

use skein_lib::Time;
use skein_world::domain::{Schedule, assert_replays};

#[test]
fn shared_domain_kit_replays_pressure_and_numbered_terminals() {
    let trace = assert_replays(17, 29, common::run);
    assert_eq!(trace.len(), 12, "six submissions and six one-time terminals were traced");
}

#[test]
fn withdrawn_delivery_is_not_delivered_and_equal_times_keep_send_order() {
    let mut schedule = Schedule::new();
    let first = schedule.send(Time::ZERO, 1_u64);
    let withdrawn = schedule.send(Time::ZERO, 2_u64);
    let _third = schedule.send(Time::ZERO, 3_u64);
    assert_eq!(schedule.withdraw(withdrawn), Some(2), "withdrawal returns ownership");
    assert_eq!(schedule.withdraw(withdrawn), None, "withdrawal has one terminal");
    assert_eq!(schedule.next(Time::ZERO), Some(1), "first equal-time delivery stays first");
    assert_eq!(schedule.withdraw(first), None, "a delivered token is stale");
    assert_eq!(schedule.next(Time::ZERO), Some(3), "later equal-time delivery stays later");
    assert!(schedule.is_empty(), "all deliveries ended by consumption or withdrawal");
}
