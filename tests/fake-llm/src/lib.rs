//! Memory consumer tests of the neutral fake LLM domain
//! (domain/session.md, sections 4, 10 and 12; testing-strategy.md, sections 2.2
//! and 6; programming-model.md, section 6.3). This package has no runtime
//! state or public entrances: its test binaries drive the fake's typed step
//! and fire boundaries with injected time, seed and bounded payloads.
//! The shared heap meter checks admission and response-generation envelopes;
//! it knows no agent state or live provider/network behavior.
