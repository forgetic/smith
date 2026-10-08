//! Local durable host decisions translated for a spawned agent's channel
//! (protocol/hosts.md, sections 5.3 and 5.6; domain/host.md, section 8).
//! This module keeps no state or credential bytes. Its entry points move a
//! saved call name and answer into the host domain's parallel sealed types.

use alloc::boxed::Box;
use skein_lib::List;
use smith_domain::{Answered, run};
use smith_host_domain::{self as host, channel};

/// A sealed local terminal did not fit the host's matching sealed vocabulary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BridgeError {
    /// One constructor refused a receipt or feedback value.
    Terminal,
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

    use super::{delivery_to_host, name_to_host, saved_reply_to_host};

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
