//! Local durable host decisions translated for a spawned agent's channel
//! (protocol/hosts.md, sections 5.3 and 5.6; domain/host.md, section 8).
//! This module keeps no state or credential bytes. Its entry points move a
//! saved call name and answer into the host domain's parallel sealed types.

use alloc::boxed::Box;
use skein_lib::{List, Reader};
use smith_domain::{Answered, run};
use smith_host_domain::{self as host, channel};
use smith_local_domain::{ExternalFinal, ExternalStart};
use smith_protocol_channel as protocol;

/// A sealed local terminal did not fit the host's matching sealed vocabulary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BridgeError {
    /// One constructor refused a receipt or feedback value.
    Terminal,
    /// The configured wire charter is not the local policy's charter.
    Charter,
    /// A saved turn does not encode under the configured endpoint table.
    Transcript,
    /// Workspace paths or grant values do not correspond to their domain names.
    Values,
    /// The result body is malformed or does not have the expected form.
    Result,
}

/// Keep the local host's display classification without inventing token usage
/// absent from the host channel's scalar spend.
pub fn answer_to_local(answer: channel::Answer) -> Result<ExternalFinal, BridgeError> {
    match answer.result {
        channel::RunResult::Accepted { outcome } => Ok(ExternalFinal::Accepted { outcome: decode_declared(&outcome)? }),
        channel::RunResult::Parked => Ok(ExternalFinal::Parked),
        channel::RunResult::Refused { refusal: _ } => Ok(ExternalFinal::Refused),
        channel::RunResult::Failed { failure } => match failure {
            channel::RunFailure::Cancelled => Ok(ExternalFinal::Cancelled),
            channel::RunFailure::Transcript(_) => Ok(ExternalFinal::TranscriptRefused),
            channel::RunFailure::Model(_)
            | channel::RunFailure::Budget(_)
            | channel::RunFailure::Policy(_)
            | channel::RunFailure::Stale => Ok(ExternalFinal::Failed),
        },
    }
}

/// Decode the accepted outcome body after the host channel has bounded it.
pub fn decode_declared(bytes: &[u8]) -> Result<run::outcome::Declared, BridgeError> {
    let Ok(record) = smith_charter::RunResult::decode(&smith_charter::CEILINGS, &mut Reader::new(bytes)) else {
        return Err(BridgeError::Result);
    };
    let fields = decode_fields(record.fields())?;
    let declared = match record.form() {
        smith_charter::Form::Change => {
            if record.label().is_some() || !record.text().is_empty() || !record.items().is_empty() {
                return Err(BridgeError::Result);
            }
            run::outcome::Declared::Change(run::outcome::Change { fields })
        }
        smith_charter::Form::Report => {
            if record.label().is_some() || !record.items().is_empty() {
                return Err(BridgeError::Result);
            }
            run::outcome::Declared::Report(run::outcome::Report { text: Box::from(record.text()), fields })
        }
        smith_charter::Form::Failure => {
            if record.label().is_some() || !record.items().is_empty() {
                return Err(BridgeError::Result);
            }
            run::outcome::Declared::Failure(run::outcome::DeclaredFailure { reason: Box::from(record.text()), fields })
        }
        smith_charter::Form::Verdict => {
            let Some(label) = record.label() else { return Err(BridgeError::Result) };
            let mut items = List::with_capacity(record.items().len());
            for item in record.items() {
                let value = run::outcome::Item { kind: Box::from(item.kind()), fields: decode_fields(item.fields())? };
                if items.push(value).is_err() {
                    return Err(BridgeError::Result);
                }
            }
            run::outcome::Declared::Verdict(run::outcome::Verdict {
                name: label.clone(),
                text: Box::from(record.text()),
                fields,
                items: items.into_boxed(),
            })
        }
    };
    Ok(declared)
}

/// Decode the channel's field-only Deliver ask (protocol/channel.md, section 5).
pub fn decode_change(bytes: &[u8]) -> Result<run::outcome::Change, BridgeError> {
    let mut reader = Reader::new(bytes);
    let Ok(record) = smith_channel::DeliverAsk::decode(&smith_channel::CEILINGS, &mut reader) else {
        return Err(BridgeError::Result);
    };
    if reader.remaining() != 0 {
        return Err(BridgeError::Result);
    }
    let mut fields = List::with_capacity(record.fields().len());
    for field in record.fields() {
        if fields.push(run::outcome::Field { name: Box::from(field.name()), value: Box::from(field.text()) }).is_err() {
            return Err(BridgeError::Result);
        }
    }
    Ok(run::outcome::Change { fields: fields.into_boxed() })
}

fn decode_fields(source: &List<smith_charter::Field>) -> Result<Box<[run::outcome::Field]>, BridgeError> {
    let mut fields = List::with_capacity(source.len());
    for field in source {
        let item = run::outcome::Field { name: Box::from(field.name()), value: Box::from(field.text()) };
        if fields.push(item).is_err() {
            return Err(BridgeError::Result);
        }
    }
    Ok(fields.into_boxed())
}

/// Host start and its parallel paths and credential envelopes, in domain order.
#[derive(Debug)]
pub struct PreparedStart {
    pub start: host::Start,
    pub paths: Box<[Box<[u8]>]>,
    pub credentials: Box<[Box<[u8]>]>,
}

/// Check the configured charter and move a local run into the spawned host's Start.
pub fn prepare_start(
    source: ExternalStart,
    charter: Box<[u8]>,
    endpoints: &protocol::Endpoints,
    paths: Box<[Box<[u8]>]>,
    credentials: Box<[Box<[u8]>]>,
) -> Result<PreparedStart, BridgeError> {
    let Ok(decoded) = protocol::decode_charter(&charter, &smith_charter::CEILINGS, endpoints) else {
        return Err(BridgeError::Charter);
    };
    if decoded != source.charter {
        return Err(BridgeError::Charter);
    }
    if source.grants.len() != credentials.len() {
        return Err(BridgeError::Values);
    }
    let (workspace, directories) = match source.workspace {
        Some(workspace) => {
            if workspace.directories.len() != paths.len() {
                return Err(BridgeError::Values);
            }
            let Ok(count) = u32::try_from(workspace.directories.len()) else {
                return Err(BridgeError::Values);
            };
            let mut directories = List::with_capacity(count);
            for directory in workspace.directories {
                let item = channel::Directory {
                    name: directory.name,
                    writable: directory.writable,
                    git: directory.git,
                    conflicts: directory.conflicts,
                };
                if directories.push(item).is_err() {
                    return Err(BridgeError::Values);
                }
            }
            (Some(skein_lib::Token::new(1)), directories.into_boxed())
        }
        None => {
            if !paths.is_empty() {
                return Err(BridgeError::Values);
            }
            (None, Box::<[channel::Directory]>::default())
        }
    };
    let transcript = match source.transcript {
        Some(history) => {
            let Ok(count) = u32::try_from(history.turns.len()) else {
                return Err(BridgeError::Transcript);
            };
            let mut turns = List::with_capacity(count);
            for turn in &history.turns {
                let Ok(bytes) = protocol::encode_turn(turn, &smith_transcript::CEILINGS, endpoints) else {
                    return Err(BridgeError::Transcript);
                };
                if turns.push(bytes).is_err() {
                    return Err(BridgeError::Transcript);
                }
            }
            Some(turns.into_boxed())
        }
        None => None,
    };
    let Ok(count) = u32::try_from(source.answered.len()) else {
        return Err(BridgeError::Terminal);
    };
    let mut answered = List::with_capacity(count);
    for call in source.answered {
        let record = channel::AnsweredCall {
            name: name_to_host(call.name),
            tool: call.tool,
            reply: saved_reply_to_host(call.answer)?,
        };
        if answered.push(record).is_err() {
            return Err(BridgeError::Terminal);
        }
    }
    let Ok(count) = u32::try_from(source.grants.len()) else {
        return Err(BridgeError::Values);
    };
    let mut grants = List::with_capacity(count);
    for grant in source.grants {
        let item =
            channel::Grant { account: grant.name.account, generation: grant.name.generation, valid: grant.valid };
        if grants.push(item).is_err() {
            return Err(BridgeError::Values);
        }
    }
    Ok(PreparedStart {
        start: host::Start {
            messages: Box::default(),
            logical_run: skein_lib::Token::new(1),
            activation: source.activation,
            workspace,
            charter,
            transcript,
            answered: answered.into_boxed(),
            directories,
            grants: grants.into_boxed(),
        },
        paths,
        credentials,
    })
}

/// Preserve one transcript-derived name across the local and host domains.
#[must_use]
pub const fn name_to_host(name: run::CallName) -> channel::CallName {
    channel::CallName { activation: name.activation, completion: name.completion, position: name.position }
}

/// Carry a settled local answer into the host's saved-call record.
pub fn saved_reply_to_host(answer: Answered) -> Result<channel::SavedReply, BridgeError> {
    match answer {
        Answered::Host(answer) => {
            Ok(channel::SavedReply::Host { error: answer.error(), body: Box::from(answer.text()) })
        }
        Answered::Delivery(delivery) => Ok(channel::SavedReply::Delivery(Box::new(delivery_to_host(*delivery)?))),
        Answered::TooLarge => Ok(channel::SavedReply::TooLarge),
    }
}

/// Preserve a durable local delivery terminal at the spawned-agent boundary.
pub fn delivery_to_host(delivery: run::Delivery) -> Result<host::Delivery, BridgeError> {
    match delivery {
        run::Delivery::Delivered(delivered) => {
            let Ok(count) = u32::try_from(delivered.receipts().len()) else {
                return Err(BridgeError::Terminal);
            };
            let mut receipts = List::with_capacity(count);
            for receipt in delivered.receipts() {
                let item =
                    host::Receipt::new(receipt.directory(), Box::from(receipt.text())).ok_or(BridgeError::Terminal)?;
                if receipts.push(item).is_err() {
                    return Err(BridgeError::Terminal);
                }
            }
            let value = host::Delivered::new(receipts.into_boxed()).ok_or(BridgeError::Terminal)?;
            Ok(host::Delivery::Delivered(value))
        }
        run::Delivery::Nothing => Ok(host::Delivery::Nothing),
        run::Delivery::Refused(refused) => {
            let marker = match refused.marker() {
                Some(marker) => {
                    Some(host::Marker::new(marker.directory(), Box::from(marker.path())).ok_or(BridgeError::Terminal)?)
                }
                None => None,
            };
            let value =
                host::DeliveryRefusal::new(marker, Box::from(refused.explanation())).ok_or(BridgeError::Terminal)?;
            Ok(host::Delivery::Refused(value))
        }
        run::Delivery::Failed(failed) => {
            let reason = match failed.reason {
                run::DeliveryReason::Unreachable => host::DeliveryReason::Unreachable,
                run::DeliveryReason::RefusedByTarget => host::DeliveryReason::RefusedByTarget,
                run::DeliveryReason::TimedOut => host::DeliveryReason::TimedOut,
                run::DeliveryReason::Broken => host::DeliveryReason::Broken,
                run::DeliveryReason::TooLarge => host::DeliveryReason::TooLarge,
                run::DeliveryReason::Missing => host::DeliveryReason::Missing,
                run::DeliveryReason::Busy => host::DeliveryReason::Busy,
                run::DeliveryReason::Unavailable => host::DeliveryReason::Unavailable,
                run::DeliveryReason::Cancelled => host::DeliveryReason::Cancelled,
                run::DeliveryReason::Unknown => host::DeliveryReason::Unknown,
            };
            Ok(host::Delivery::Failed(host::DeliveryFailure {
                directory: failed.directory,
                reason,
                diagnostic: host::Diagnostic::new(failed.diagnostic.output(), failed.diagnostic.cut()),
            }))
        }
        run::Delivery::Stale => Ok(host::Delivery::Stale),
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use smith_domain::run;
    use smith_host_domain as host;

    use super::{
        answer_to_local, decode_change, decode_declared, delivery_to_host, name_to_host, prepare_start,
        saved_reply_to_host,
    };

    #[test]
    fn a_spawned_last_word_keeps_its_local_display_class() {
        let final_word =
            answer_to_local(host::Answer { read: None, turns: 3, spent: 29, result: host::RunResult::Parked })
                .expect("parked word");
        let smith_local_domain::ExternalFinal::Parked = final_word else { panic!("parked local chat") };
        let final_word = answer_to_local(host::Answer {
            read: None,
            turns: 1,
            spent: 7,
            result: host::RunResult::Failed { failure: host::RunFailure::Cancelled },
        })
        .expect("cancelled word");
        let smith_local_domain::ExternalFinal::Cancelled = final_word else { panic!("cancelled local chat") };
    }

    #[test]
    fn a_change_and_report_decode_from_the_agent_result_body() {
        let change = run::outcome::Declared::Change(run::outcome::Change {
            fields: Box::from([run::outcome::Field {
                name: Box::from(&b"title"[..]),
                value: Box::from(&b"Commit"[..]),
            }]),
        });
        let bytes = smith_protocol_channel::encode_result(&change, &smith_charter::CEILINGS).expect("change bytes");
        assert_eq!(decode_declared(&bytes), Ok(change));
        let limits = &smith_channel::CEILINGS;
        let mut fields = skein_lib::List::with_capacity(1);
        fields
            .push(
                smith_channel::Field::new(
                    limits,
                    smith_channel::FieldParts { name: Box::from(&b"title"[..]), text: Box::from(&b"Commit"[..]) },
                )
                .expect("field"),
            )
            .expect("one field");
        let ask = smith_channel::DeliverAsk::new(limits, smith_channel::DeliverAskParts { fields }).expect("ask");
        let mut writer = skein_lib::Writer::new(usize::try_from(ask.measure()).expect("ask measure"));
        ask.encode(&mut writer).expect("ask encode");
        let bytes = writer.finish();
        assert_eq!(
            decode_change(&bytes),
            Ok(run::outcome::Change {
                fields: Box::from([run::outcome::Field {
                    name: Box::from(&b"title"[..]),
                    value: Box::from(&b"Commit"[..])
                }]),
            })
        );

        let report = run::outcome::Declared::Report(run::outcome::Report {
            text: Box::from(&b"finished"[..]),
            fields: Box::new([]),
        });
        let bytes = smith_protocol_channel::encode_result(&report, &smith_charter::CEILINGS).expect("report bytes");
        assert_eq!(decode_declared(&bytes), Ok(report));
        assert_eq!(decode_change(&bytes), Err(super::BridgeError::Result));
    }

    #[test]
    fn a_local_start_keeps_the_configured_wire_charter_and_run_identity() {
        let limits = &smith_charter::CEILINGS;
        let families = smith_charter::Families::new(limits, smith_charter::FamiliesParts { skein_bools: [false; 4] })
            .expect("families");
        let tools = smith_charter::Tools::new(
            limits,
            smith_charter::ToolsParts { families, wait: false, deliver: None, host: skein_lib::List::with_capacity(0) },
        )
        .expect("tools");
        let contract = smith_charter::Contract::new(
            limits,
            smith_charter::ContractParts {
                report: None,
                verdicts: skein_lib::List::with_capacity(0),
                change: None,
                failure: None,
            },
        )
        .expect("contract");
        let prices =
            smith_charter::Prices::new(limits, smith_charter::PricesParts { input: 0, cached: 0, output: 0, unit: 0 })
                .expect("prices");
        let main = smith_charter::Llm::new(
            limits,
            smith_charter::LlmParts {
                endpoint: Box::from(&b""[..]),
                model: Box::from(&b""[..]),
                max_tokens: 0,
                prices,
            },
        )
        .expect("main");
        let record = smith_charter::Charter::new(
            limits,
            smith_charter::CharterParts {
                instructions: Box::from(&b""[..]),
                brief: skein_lib::List::with_capacity(0),
                tools,
                contract,
                conventions: None,
                budget: smith_charter::Budget::new(
                    limits,
                    smith_charter::BudgetParts { turns: 0, spend: 0, time: skein_lib::Duration::ZERO },
                )
                .expect("budget"),
                main,
                models: skein_lib::List::with_capacity(0),
                waiting: skein_lib::Duration::ZERO,
                resume: false,
            },
        )
        .expect("charter");
        let mut writer = skein_lib::Writer::new(usize::try_from(record.measure()).expect("bounded charter"));
        record.encode(&mut writer).expect("measured bytes");
        let bytes = writer.finish();
        let mut entries = skein_lib::List::with_capacity(1);
        entries
            .push(smith_protocol_channel::Endpoint { name: Box::from(&b""[..]), number: 2, dialect: 1, account: 0 })
            .expect("one endpoint");
        let endpoints = smith_protocol_channel::Endpoints::new(entries);
        let charter = smith_protocol_channel::decode_charter(&bytes, &smith_charter::CEILINGS, &endpoints)
            .expect("golden charter");
        let start = smith_local_domain::ExternalStart {
            activation: 7,
            charter,
            workspace: None,
            transcript: None,
            answered: Box::new([]),
            grants: Box::new([]),
        };
        let prepared =
            prepare_start(start, bytes.clone(), &endpoints, Box::new([]), Box::new([])).expect("matching local start");
        assert_eq!(prepared.start.activation, 7);
        assert_eq!(prepared.start.logical_run, skein_lib::Token::new(1));
        assert_eq!(prepared.start.charter, bytes);
        assert!(prepared.start.transcript.is_none());
    }

    #[test]
    fn a_saved_delivery_retains_its_name_receipt_and_failure_tail() {
        let name = run::CallName { activation: 9, completion: 3, position: 1 };
        assert_eq!(name_to_host(name), host::channel::CallName { activation: 9, completion: 3, position: 1 });
        let receipt = run::Receipt::new(0, Box::from(&b"commit abc"[..])).expect("receipt");
        let delivered = run::Delivered::new(Box::from([receipt])).expect("delivery");
        let saved =
            saved_reply_to_host(smith_domain::Answered::Delivery(Box::new(run::Delivery::Delivered(delivered))))
                .expect("saved answer");
        let host::channel::SavedReply::Delivery(answer) = saved else { panic!("delivery answer") };
        let host::Delivery::Delivered(answer) = *answer else { panic!("landed terminal") };
        assert_eq!(answer.receipts()[0].text(), b"commit abc");

        let failed = run::DeliveryFailure {
            directory: 1,
            reason: run::DeliveryReason::TimedOut,
            diagnostic: run::Diagnostic::new(b"git timed out", 7),
        };
        let converted = delivery_to_host(run::Delivery::Failed(failed)).expect("failed terminal");
        let host::Delivery::Failed(converted) = converted else { panic!("failure") };
        assert_eq!(converted.reason, host::DeliveryReason::TimedOut);
        assert_eq!(converted.diagnostic.output(), b"git timed out");
        assert_eq!(converted.diagnostic.cut(), 7);
    }
}
