//! Smith's application vocabulary over the actual shared LLM Client.
//!
//! Preparation checks the receiving contract before constructing one Client.
//! Its context retains the exact served and application declarations until an
//! actual terminal. Pure translations preserve block order, full replay,
//! provider IDs, usage and bounded failure evidence. The caller drives the
//! Client's stream and retains it through actual reuse or lower settlement.
//! There is no provider grammar, credential exchange, application scheduling
//! or retry policy here. Non-host schemas and decoded values come from the
//! caller's explicit application codec, without callbacks or application traits.
//!
//! [`prepare`] constructs the Client and Context; [`prompt()`] translates only
//! the prompt. [`completion()`], [`failed`] and [`cancelled`] consume the Context
//! once for an actual terminal, while [`refusal`] reports preparation refusal
//! before wire work. The caller separately keeps and drives the Client through
//! Reusable or Close/Closed settlement.
//! Contract: programming-model.md, sections 4.4 and 6.3.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod completion;
mod decode;
mod failure;
mod limits;
mod prompt;
mod tools;
mod types;

pub use completion::completion;
pub use decode::decode;
pub use failure::{cancelled, failed, refusal};
pub use limits::{Limits, Receiving, completion_worst_case, worst_case};
pub use prompt::{prepare, prompt};
pub use skein_llm::Error;
pub use tools::schemas;
pub use types::{Context, Input, Prepared, ResolvedCall, ResultText, ToolKind, ToolSchema};
