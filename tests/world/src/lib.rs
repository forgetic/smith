//! Consumer tests for skein's shared domain-world kit
//! (testing-strategy.md, sections 2.2, 6 and 7; domain/README.md, section 4).
//! This crate keeps no agent state, fakes or harness implementation. Its
//! integration tests exercise the shared schedule, flow-control stage,
//! terminal ledger, observation referee and replay contracts. The domain
//! worlds added with smith's implementations keep their own settings and
//! independent expectations and consume `skein_world::domain` directly.

#![forbid(unsafe_code)]
