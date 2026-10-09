//! Outside relay history judge: immutable operations, settled recovery and exact
//! first-decision feedback. One executable call retains its observed assistant
//! message/block origin and complete ID/name/input; IDs in other historical turns
//! confer no authority. Entrances observe calls, submissions, terminals and the
//! exact locally paired prompt; the judge never reads private domain state.
//! Contract: domain/run.md, sections 5.2 and 13; scratch/client.md, sections 3–5;
//! testing-strategy.md, section 7.

use skein_lib::{Time, Token};
use smith_domain::{llm, run};

/// Public observations retained independently of the implementation.
/// Contract: domain/run.md, sections 5.2 and 13.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Submission {
    /// Parent-supplied logical scope. Contract: domain/run.md, section 5.2.
    pub host_run: Token,
    /// Actual callback generation and attempt. Contract: domain/run.md, section 5.2.
    pub relay: run::RelayName,
    /// Immutable durable transcript name. Contract: domain/run.md, section 5.2.
    pub name: run::CallName,
    /// Whole opaque tool identifier. Contract: domain/run.md, section 5.2.
    pub tool: Box<[u8]>,
    /// Declared scheduling effect. Contract: domain/run.md, section 5.2.
    pub effect: run::HostEffect,
    /// Whole protocol-attested input. Contract: domain/run.md, section 5.2.
    pub input: run::HostInput,
    /// Actual admission time. Contract: domain/run.md, section 5.2.
    pub at: Time,
    /// Effective bounded relay deadline. Contract: domain/run.md, section 5.2.
    pub deadline: Time,
}

/// Judge knows public submissions and terminals, never domain state.
/// Contract: domain/run.md, sections 5.2, 13 and 14.
#[derive(Debug, Default)]
pub struct History {
    submissions: Vec<Submission>,
    live: Option<run::RelayName>,
    previous_terminal: Option<Time>,
    answered: Option<run::HostAnswer>,
    uncertain: bool,
    reported_too_large: bool,
    feedback: bool,
    provider_call: Option<ProviderCall>,
    shutdown: Option<Time>,
}

#[derive(Debug)]
struct ProviderCall {
    message: u32,
    position: u32,
    id: Box<[u8]>,
    name: Box<[u8]>,
    input: Box<[u8]>,
}

impl History {
    /// Admission rejects live duplicates and mutable recovery input.
    /// Contract: domain/run.md, section 5.2.
    ///
    /// # Errors
    /// Rejects duplicate live relays, recovery after answer/feedback/shutdown,
    /// expired deadlines, a relay differing from observed provider name/input,
    /// mutated recovery input or nonsequential callbacks and chronology.
    pub fn submit(&mut self, submission: Submission) -> Result<(), &'static str> {
        if self.live.is_some() {
            return Err("duplicate live relay");
        }
        if self.feedback {
            return Err("recovery after logical feedback");
        }
        if self.shutdown.is_some() {
            return Err("recovery after observed shutdown");
        }
        if self.answered.is_some() {
            return Err("recovery after actual answer");
        }
        if submission.at >= submission.deadline {
            return Err("unbounded or expired relay");
        }
        if let Some(call) = &self.provider_call
            && (submission.tool != call.name || submission.input.bytes() != call.input.as_ref())
        {
            return Err("relay differs from observed provider call");
        }
        if let Some(first) = self.submissions.first() {
            if (first.host_run, first.name, &first.tool, first.effect, &first.input)
                != (submission.host_run, submission.name, &submission.tool, submission.effect, &submission.input)
            {
                return Err("recovery changed immutable operation");
            }
            let prior = self.submissions.last().expect("first exists");
            if submission.relay == prior.relay
                || submission.relay.attempt != prior.relay.attempt.checked_add(1).expect("bounded attempts")
            {
                return Err("recovery callback is not a new attempt");
            }
            if self.previous_terminal.is_none_or(|at| submission.at <= at) {
                return Err("recovery precedes terminal/backoff");
            }
        } else if submission.relay.attempt != 1 {
            return Err("first attempt is not one");
        }
        self.live = Some(submission.relay);
        self.submissions.push(submission);
        Ok(())
    }

    /// Withdrawal is only a request; it retains the live obligation.
    /// Contract: domain/run.md, section 5.2.
    ///
    /// # Errors
    /// Rejects a withdrawal naming an attempt that is not currently live.
    pub fn withdraw(&self, relay: run::RelayName) -> Result<(), &'static str> {
        if self.live == Some(relay) { Ok(()) } else { Err("withdrawal names no live relay") }
    }

    /// Actual terminal closes this attempt and preserves any first decision.
    /// Contract: domain/run.md, section 5.2.
    ///
    /// # Errors
    /// Rejects unmatched callbacks, terminals before admission, and changed
    /// recorded answers for the same durable operation.
    pub fn terminal(&mut self, at: Time, relay: run::RelayName, reply: &run::HostReply) -> Result<(), &'static str> {
        if self.live != Some(relay) {
            return Err("terminal names no live relay");
        }
        if at < self.submissions.last().expect("live relay has an observed submission").at {
            return Err("terminal precedes live admission");
        }
        match reply {
            run::HostReply::Answered(answer) => {
                if self.answered.as_ref().is_some_and(|first| first != answer) {
                    return Err("second decision for durable name");
                }
                self.answered = Some(answer.clone());
                self.uncertain = false;
            }
            run::HostReply::Unanswered(_) | run::HostReply::Withdrawn => self.uncertain = true,
            run::HostReply::TooLarge => self.reported_too_large = true,
            run::HostReply::Busy => {}
        }
        self.live = None;
        self.previous_terminal = Some(at);
        Ok(())
    }

    /// Conversation feedback must wait for actual terminal and preserve text.
    /// Contract: domain/run.md, section 5.2.
    ///
    /// # Errors
    /// Rejects early/duplicate feedback or evidence differing from the actual
    /// terminal, including erasing uncertain effects into known predecision results.
    pub fn feedback(&mut self, result: &run::Returned) -> Result<(), &'static str> {
        if self.live.is_some() || self.feedback {
            return Err("feedback before terminal or twice");
        }
        let valid = match result {
            run::Returned::HostAnswered(answer) => self.answered.as_ref() == Some(answer),
            run::Returned::HostTooLarge { bytes, max } => self
                .answered
                .as_ref()
                .is_some_and(|answer| u32::try_from(answer.text().len()).ok() == Some(*bytes) && bytes > max),
            run::Returned::HostReportedTooLarge => self.reported_too_large,
            run::Returned::HostUnknown => self.answered.is_none() && self.uncertain,
            run::Returned::Busy | run::Returned::Cancelled | run::Returned::TimedOut => {
                self.answered.is_none() && !self.uncertain
            }
            run::Returned::Waiting
            | run::Returned::Crossed { .. }
            | run::Returned::HostRejected(_)
            | run::Returned::Delivered(_)
            | run::Returned::Nothing
            | run::Returned::DeliveryRefused(_)
            | run::Returned::Accepted
            | run::Returned::Rejected { .. }
            | run::Returned::ChecksFailed { .. }
            | run::Returned::Stale
            | run::Returned::DeliveryFailed { .. }
            | run::Returned::Answered { .. }
            | run::Returned::Unanswered { .. }
            | run::Returned::Refused { .. } => false,
        };
        if !valid {
            return Err("feedback erased or fabricated host evidence");
        }
        self.feedback = true;
        Ok(())
    }

    /// The world binds one actual `ToolUse` call decoded as a served Host ask,
    /// before the domain relays it. `message` is the next assistant index from
    /// the owning actual Complete prompt; `position` is the actual Said index.
    /// Complete original ID/name/input bytes retain that bounded call's identity
    /// until its exact local feedback, or observed shutdown and relay settlement.
    /// Other historical turns may reuse the same provider ID. This single-call
    /// story retains one origin and three caller-bounded byte fields.
    /// Contract: domain/run.md, sections 5.2 and 13; domain/session.md, section 3;
    /// scratch/client.md, sections 3–5.
    ///
    /// # Errors
    /// Rejects a second provider host-tool call in this single-operation story.
    pub fn called(
        &mut self,
        message: u32,
        position: u32,
        provider_id: Box<[u8]>,
        name: Box<[u8]>,
        input: Box<[u8]>,
    ) -> Result<(), &'static str> {
        if self.provider_call.is_some() {
            return Err("second provider host call");
        }
        self.provider_call = Some(ProviderCall { message, position, id: provider_id, name, input });
        Ok(())
    }

    fn paired_result<'a>(&self, prompt: &'a llm::Prompt) -> Result<Option<&'a llm::Returned>, &'static str> {
        let call = self.provider_call.as_ref().expect("caller has an observed provider call");
        let message = usize::try_from(call.message).expect("u32 fits usize");
        let assistant = prompt.messages.get(message).ok_or("continuation omitted host call origin")?;
        if assistant.role != llm::Role::Assistant {
            return Err("host call origin is not assistant");
        }
        let position = usize::try_from(call.position).expect("u32 fits usize");
        match assistant.content.get(position) {
            Some(llm::Block::ToolCall { id, name, input, .. })
                if *id == call.id && *name == call.name && *input == call.input => {}
            Some(_) | None => return Err("host call origin was rewritten"),
        }
        let calls = assistant
            .content
            .iter()
            .filter(|block| matches!(block, llm::Block::ToolCall { id, .. } if *id == call.id))
            .count();
        if calls != 1 {
            return Err("duplicate host call ID at origin");
        }
        let user = prompt
            .messages
            .get(message.checked_add(1).expect("bounded message origin"))
            .ok_or("continuation omitted host feedback")?;
        if user.role != llm::Role::User {
            return Err("host feedback is not paired user");
        }
        let mut result = None;
        for block in &user.content {
            match block {
                llm::Block::ToolResult { id, result: returned } if *id == call.id => {
                    if result.replace(returned).is_some() {
                        return Err("duplicate host feedback ID");
                    }
                }
                llm::Block::Text { .. }
                | llm::Block::Refusal { .. }
                | llm::Block::Opaque { .. }
                | llm::Block::ToolCall { .. }
                | llm::Block::ToolResult { .. } => {}
            }
        }
        Ok(result)
    }

    /// The world observes the next actual provider prompt and requires the
    /// retained original assistant call at its message/block origin, followed
    /// by exactly one matching result in its paired User message. Older turns
    /// with the same provider ID do not count. No feedback closes a live relay.
    /// Contract: domain/run.md, sections 5.2 and 13; domain/session.md, section 3;
    /// scratch/client.md, sections 3–5.
    ///
    /// # Errors
    /// Rejects changed/missing call origins, duplicate local call IDs,
    /// missing/duplicate paired feedback, wrong result variants,
    /// changed text/error classification or feedback before the actual terminal.
    pub fn prompt(&mut self, prompt: &llm::Prompt) -> Result<(), &'static str> {
        if self.feedback || self.provider_call.is_none() {
            return Ok(());
        }
        let result = self.paired_result(prompt)?;
        match result {
            Some(llm::Returned::Text { text, error, replay }) => {
                if self.live.is_some() || replay.is_some() {
                    return Err("feedback precedes actual terminal or invents replay metadata");
                }
                if self.reported_too_large {
                    if text.as_ref() != b"host-decided answer-too-large answer-not-shown" || !error {
                        return Err("feedback erased or fabricated reported oversized host answer");
                    }
                    self.feedback = true;
                    return Ok(());
                }
                let expected = match &self.answered {
                    Some(answer) => (answer.text(), answer.error()),
                    None if self.uncertain => (b"host-unknown".as_slice(), true),
                    None => (b"busy".as_slice(), true),
                };
                if text.as_ref() != expected.0 || *error != expected.1 {
                    return Err("feedback erased or fabricated exact host evidence");
                }
                self.feedback = true;
                Ok(())
            }
            Some(llm::Returned::Served { returned, error }) => {
                let expected_error = match returned {
                    run::Returned::HostAnswered(answer) => answer.error(),
                    run::Returned::HostTooLarge { .. }
                    | run::Returned::HostReportedTooLarge
                    | run::Returned::HostUnknown
                    | run::Returned::Busy
                    | run::Returned::Cancelled
                    | run::Returned::TimedOut => true,
                    run::Returned::Waiting
                    | run::Returned::Crossed { .. }
                    | run::Returned::HostRejected(_)
                    | run::Returned::Delivered(_)
                    | run::Returned::Nothing
                    | run::Returned::DeliveryRefused(_)
                    | run::Returned::Accepted
                    | run::Returned::Rejected { .. }
                    | run::Returned::ChecksFailed { .. }
                    | run::Returned::Stale
                    | run::Returned::DeliveryFailed { .. }
                    | run::Returned::Answered { .. }
                    | run::Returned::Unanswered { .. }
                    | run::Returned::Refused { .. } => return Err("host feedback was rewritten"),
                };
                if *error != expected_error {
                    return Err("host feedback error bit changed");
                }
                self.feedback(returned)
            }
            Some(
                llm::Returned::Withdrawn
                | llm::Returned::Owned { .. }
                | llm::Returned::Invalid { .. }
                | llm::Returned::NotRun,
            ) => Err("host feedback was rewritten"),
            None => Err("continuation omitted host feedback"),
        }
    }

    /// Record actual outside cancellation or final failed-run shutdown evidence.
    /// Contract: domain/run.md, sections 5.2 and 10.
    pub fn shutdown(&mut self, at: Time) {
        self.shutdown = self.shutdown.or(Some(at));
    }

    /// Actual outside shutdown time for chronology controls.
    /// Contract: domain/run.md, sections 5.2 and 10.
    #[must_use]
    pub const fn shutdown_at(&self) -> Option<Time> {
        self.shutdown
    }

    /// A normal continuation owes exact feedback; observed shutdown may have
    /// no next provider prompt, but it still owes every actual relay terminal.
    /// Contract: domain/run.md, sections 5.2 and 10.
    ///
    /// # Errors
    /// Rejects an outstanding actual relay, missing ordinary continuation
    /// feedback or a claimed shutdown that was never observed.
    pub fn finish(&self, continuation: bool) -> Result<(), &'static str> {
        if self.live.is_some() {
            return Err("shutdown abandoned actual relay terminal");
        }
        if !continuation && !self.submissions.is_empty() && self.shutdown.is_none() {
            return Err("shutdown was not observed");
        }
        if continuation && !self.submissions.is_empty() && !self.feedback {
            return Err("continuation omitted host feedback");
        }
        Ok(())
    }

    /// Exact observed operations for replay and immutable recovery assertions.
    /// Contract: domain/run.md, sections 5.2 and 14.
    #[must_use]
    pub fn submissions(&self) -> &[Submission] {
        &self.submissions
    }
}
