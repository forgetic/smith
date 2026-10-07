//! Bounded channel records and the version-one kind table. This crate keeps no
//! runtime state and knows no domain, credentials or host policy. Generated
//! constructors, encoders and decoders handle bodies; [`schema`] advertises
//! their largest permitted encodings. Contracts: `protocol/channel.md`,
//! sections 2-4 and 6-8.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[rustfmt::skip]
#[path = "generated/v1.rs"]
pub mod v1;

mod kinds;

pub use kinds::{KindRule, Overflow, V1, peer_terms_gap, schema};
pub use v1::{
    Accepted, AcceptedParts, Acknowledge, AcknowledgeParts, Admitted, AdmittedParts, Answer, AnswerParts, AnsweredCall,
    AnsweredCallParts, Ask, BudgetFailure, BudgetFailureValue, BudgetFailureValueParts, CEILINGS, Call, CallName,
    CallNameParts, CallParts, Cancel, CancelParts, CompletionEvidence, CompletionFailure, CompletionFault,
    CompletionFaultParts, DeliverAsk, DeliverAskParts, Delivered, DeliveredParts, Delivery, DeliveryFailure,
    DeliveryFailureParts, DeliveryReason, DeliveryRefusal, DeliveryRefusalParts, DeliveryReply, DeliveryReplyParts,
    Directory, DirectoryParts, Effect, Exhausted, ExhaustedParts, Fact, FactKind, FactParts, Failed, FailedParts,
    Field, FieldParts, Grant, GrantParts, GrantRefresh, GrantRefreshParts, GrantValue, GrantValueParts, HostAnswer,
    HostAnswerParts, HostAsk, HostAskParts, HostReply, HostReplyParts, InvalidStart, InvalidStartValue,
    InvalidStartValueParts, Limits, Long, LongDone, LongDoneParts, LongParts, Marker, MarkerParts, Message,
    MessageParts, ModelFault, ModelFaultValue, ModelFaultValueParts, Overflow as BudgetOverflow, OverflowValue,
    OverflowValueParts, PolicyFailure, PolicyFailureParts, Receipt, ReceiptParts, ReceivingLimit, ReceivingLimitValue,
    ReceivingLimitValueParts, Refused, RefusedParts, Rejected, RejectedParts, Reply, RunFailure, RunResult, SavedReply,
    Start, StartParts, StartRefusal, TranscriptRefusal, TranscriptRefusalValue, TranscriptRefusalValueParts, Turn,
    TurnParts, Waiting, WaitingParts, Window, WindowParts, Withdraw, WithdrawParts, Workspace, WorkspaceParts,
};
