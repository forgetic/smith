//! Local scenario configuration for Skein's shared loopback peer.
//! No transport or provider machinery is owned here (testing.md, section 4).
pub use skein_fake_peers::llm::Peer;
pub use smith_agent_process_world::fake::{configured as peer, queries};
