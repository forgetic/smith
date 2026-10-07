//! Bounded text for answers absent from a resumed transcript (domain/run.md, section 6).
//! The root translates each semantic answer through its live feedback renderer,
//! then gives the session one first prompt. No provider message is assembled here.

use alloc::boxed::Box;
use skein_lib::List;
use smith_domain_run as run;

use crate::{Answered, AnsweredCall, Limits};

pub(crate) fn render(answered: Box<[AnsweredCall]>, limits: &Limits) -> Option<Box<[u8]>> {
    let count = u32::try_from(answered.len()).ok()?;
    if count > limits.run.answered_calls {
        return None;
    }
    if answered.is_empty() {
        return Some(Box::default());
    }
    let mut text = List::with_capacity(limits.run.answered_bytes);
    put(&mut text, b"Earlier host answers absent from the saved transcript:\n")?;
    for AnsweredCall { name, tool, answer } in answered {
        let returned = match answer {
            Answered::Host(answer) => run::Returned::HostAnswered(answer),
            Answered::Delivery(delivery) => match *delivery {
                run::Delivery::Delivered(receipts) => run::Returned::Delivered(receipts),
                run::Delivery::Nothing => run::Returned::Nothing,
                run::Delivery::Refused(refusal) => run::Returned::DeliveryRefused(refusal),
                run::Delivery::Failed(failure) => run::Returned::DeliveryFailed { failure },
                run::Delivery::Stale => run::Returned::Stale,
            },
        };
        let feedback = crate::feedback(returned, limits.session.delegated_result_bytes).ok()?;
        put(&mut text, b"call activation=")?;
        number(&mut text, name.activation)?;
        put(&mut text, b" completion=")?;
        number(&mut text, u64::from(name.completion))?;
        put(&mut text, b" position=")?;
        number(&mut text, u64::from(name.position))?;
        put(&mut text, b" tool=")?;
        put(&mut text, &tool)?;
        put(&mut text, if feedback.error { b" error: " } else { b" result: " })?;
        put(&mut text, &feedback.text)?;
        put(&mut text, b"\n")?;
    }
    put(&mut text, b"\n")?;
    Some(text.into_boxed())
}

fn put(text: &mut List<u8>, bytes: &[u8]) -> Option<()> {
    for byte in bytes {
        text.push(*byte).ok()?;
    }
    Some(())
}

fn number(text: &mut List<u8>, mut value: u64) -> Option<()> {
    let mut digits = [0_u8; 20];
    let mut count = 0_usize;
    for _ in 0_u8..20_u8 {
        *digits.get_mut(count)? = b'0'.checked_add(u8::try_from(value % 10).ok()?)?;
        count = count.checked_add(1)?;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    for digit in digits.get(..count)?.iter().rev() {
        text.push(*digit).ok()?;
    }
    Some(())
}
