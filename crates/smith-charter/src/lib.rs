//! Bounded charter and result records; this crate retains no runtime state and
//! knows no domain, endpoint configuration or host policy. Constructors,
//! encoders and decoders are generated from `protocol/charter.md`, sections 2-6.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[rustfmt::skip]
#[path = "generated/v1.rs"]
pub mod v1;

pub use v1::{
    Budget, BudgetParts, CEILINGS, ChangeRule, ChangeRuleParts, Charter, CharterParts, Contract, ContractParts,
    Conventions, ConventionsParts, Effect, Families, FamiliesParts, Field, FieldParts, FieldRule, FieldRuleParts, Form,
    HostTool, HostToolParts, Item, ItemKind, ItemKindParts, ItemParts, Llm, LlmParts, Prices, PricesParts, RunResult,
    RunResultParts, Section, SectionParts, TextRule, TextRuleParts, Tools, ToolsParts, VerdictRule, VerdictRuleParts,
};
