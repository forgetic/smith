//! Tickets: the values a session names but cannot hold, kept for it by its
//! opener's side, as the root domain keeps them for a real session: the
//! tools the opener serves, the calls the LLM made to them. Concrete answers move directly
//! into the session. A session's tickets are freed when it ends, and resolving one
//! after that is a bug the world catches.

use std::collections::BTreeMap;

use skein_lib::Token;
use smith_domain_tools::Effect;

/// The tools the fake opener serves: a finish, which writes, and a lookup,
/// which only reads and is slow.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
pub const SERVED: [(&[u8], Effect, &[u8]); 2] =
    [(b"finish", Effect::Write, br#"{"summary":"string"}"#), (b"lookup", Effect::Read, br#"{"path":"string"}"#)];

/// What a ticket names.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Ticketed {
    /// A tool the opener serves.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    Tool {
        /// Exact byte name of the opener-served tool.
        ///
        /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
        name: &'static [u8],
        /// Whether this delegated call may write, used to serialize writers.
        ///
        /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
        effect: Effect,
        /// Validated bounded JSON schema supplied for the offered tool.
        ///
        /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
        schema: &'static [u8],
    },
    /// A call the LLM made to such a tool, with the arguments it wrote.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    Call {
        /// Content-free classification of the checkout tool called.
        ///
        /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
        tool: &'static [u8],
        /// Owned model-provided arguments retained for this opener-served call.
        ///
        /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
        arguments: Box<[u8]>,
    },
}

/// The tickets of every session, by the opener's name for the session each
/// is for.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
#[derive(Default, Debug)]
pub struct Tickets {
    issued: u64,
    held: BTreeMap<Token, (u64, Ticketed)>,
}

impl Tickets {
    /// A new ticket for `value`, held for the session of `opener`.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub fn issue(&mut self, opener: u64, value: Ticketed) -> Token {
        self.issued += 1;
        let ticket = Token::new(self.issued);
        self.held.insert(ticket, (opener, value));
        ticket
    }

    /// What `ticket` names; only while its session lives.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    #[must_use]
    pub fn resolve(&self, ticket: Token) -> &Ticketed {
        let (_, value) = self.held.get(&ticket).expect("a ticket is resolved only while its session lives");
        value
    }

    /// Frees the tickets of the session of `opener`, which has ended.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub fn free(&mut self, opener: u64) {
        self.held.retain(|_, (held, _)| *held != opener);
    }

    /// Tickets held, for the sessions that live.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    #[must_use]
    pub fn len(&self) -> usize {
        self.held.len()
    }

    /// Whether no live ticket values remain retained.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}
