//! Message terminal oracle over actual content-free run output. Skein owns
//! judgments and liveness. Contract: domain/run.md, sections 6 and 13.

use skein_lib::{Duration, Time, Token};
use skein_world::domain::{Expectations, Judge, Referee, Verdict};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// One actual message fact or the final boundary fence.
#[derive(Clone, Copy, Debug)]
pub enum Seen {
    /// The run admitted this name into its inbox.
    Received { name: Token },
    /// A told turn covered this accepted name.
    Read { name: Token },
    /// The answer ended this accepted name unread.
    Unread { name: Token },
    /// A told turn carried its current fence.
    Fence { read: Option<Token> },
    /// The final answer carried its last told fence.
    Answer { read: Option<Token> },
}

/// External arrival order and terminals for one admitted activation.
#[derive(Default)]
pub struct Meeting {
    queued: VecDeque<Token>,
    terminal: BTreeSet<Token>,
    read: Option<Token>,
    fence: Option<Token>,
    answered: bool,
}

impl Expectations for Meeting {
    type Seen = Seen;
    type Name = &'static str;
    type Stimulus = ();

    fn observe(&mut self, seen: Seen, judge: &mut Judge<Self::Name, Self::Stimulus>) {
        judge.check(!self.answered, "no accepted message output follows the answer");
        match seen {
            Seen::Received { name } => {
                judge.check(
                    !self.queued.contains(&name) && self.read != Some(name),
                    "accepted names are not already in use",
                );
                self.terminal.remove(&name);
                self.queued.push_back(name);
                judge.rearm("message terminals", Duration::from_secs(100_000));
            }
            Seen::Read { name } => {
                judge.check(self.queued.pop_front() == Some(name), "read covers the oldest accepted message");
                judge.check(self.terminal.insert(name), "every accepted message has one terminal");
                self.read = Some(name);
            }
            Seen::Unread { name } => {
                judge.check(self.queued.pop_front() == Some(name), "unread covers the retained ordered prefix");
                judge.check(self.terminal.insert(name), "every accepted message has one terminal");
            }
            Seen::Fence { read } => {
                judge.check(read == self.read, "fences follow actual read terminals in arrival order");
                self.fence = read;
            }
            Seen::Answer { read } => {
                judge.check(self.queued.is_empty(), "every accepted message ends by the answer");
                judge.check(read == self.fence && read == self.read, "answer retains the last actual told fence");
                judge.withdraw(&"message terminals");
                self.answered = true;
            }
        }
    }
}

/// Judge actual per-activation observations through Skein's referee.
#[must_use]
pub fn judge(seen: &[(Time, Token, Seen)]) -> Vec<Verdict> {
    let mut meetings = BTreeMap::new();
    for (at, run, seen) in seen {
        let meeting = meetings.entry(*run).or_insert_with(|| Referee::new(Meeting::default()));
        meeting.observe(*at, *seen, &mut Vec::new());
    }
    meetings.values().map(Referee::verdict).collect()
}
