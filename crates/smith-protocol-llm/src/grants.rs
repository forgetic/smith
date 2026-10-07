//! Bounded credential generations read only when a call starts.
//! The table keeps two generations per account so a refresh does not change
//! the credential already handed to an in-flight connection. It never sends
//! values into the domain, transcript, fact or trace.
//! Contract: protocol/channel.md, section 6; protocol/llm.md, section 7.

use core::fmt;

use skein_lib::{Map, Time, bytes};
use skein_llm::Credential;
use smith_domain::GrantName;

/// Why a grant update or lookup cannot supply a call.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GrantError {
    /// Account, generation or value exceeds configured bounds.
    Bounds,
    /// The generation does not advance that account.
    Stale,
    /// Capacity could not retain the update.
    Full,
    /// No value for the requested generation exists.
    Missing,
    /// The requested generation's validity has ended.
    Lapsed,
}

struct Entry {
    credential: Credential,
    lapses: Time,
}

impl fmt::Debug for Entry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Entry").field("credential", &"[redacted]").field("lapses", &self.lapses).finish()
    }
}

/// At most two values per configured account, bounded before admission.
#[derive(Debug)]
pub struct Grants {
    values: Map<(u32, u64), Entry>,
    accounts: u32,
    value_bytes: u32,
}

impl Grants {
    /// Reserve two generations per account; the value cap includes bearer and
    /// provider account-id bytes.
    pub fn new(accounts: u32, value_bytes: u32) -> Result<Grants, GrantError> {
        let capacity = accounts.checked_mul(2).ok_or(GrantError::Bounds)?;
        Ok(Grants { values: Map::with_capacity(capacity), accounts, value_bytes })
    }

    /// Keep a newer generation and evict the oldest only when the account
    /// already has two. A stale refresh never replaces a value in flight.
    pub fn grant(&mut self, name: GrantName, credential: Credential, lapses: Time) -> Result<(), GrantError> {
        let bytes = credential.access_token.len().checked_add(credential.account_id.len()).ok_or(GrantError::Bounds)?;
        if name.account >= self.accounts
            || name.generation == 0
            || bytes > usize::try_from(self.value_bytes).expect("u32 fits usize")
        {
            return Err(GrantError::Bounds);
        }
        let mut count = 0_u32;
        let mut oldest = None;
        let mut newest = 0_u64;
        for ((account, generation), _) in &self.values {
            if *account == name.account {
                count = count.checked_add(1).ok_or(GrantError::Bounds)?;
                if oldest.is_none() {
                    oldest = Some(*generation);
                }
                newest = newest.max(*generation);
            }
        }
        if count > 0 && name.generation <= newest {
            return Err(GrantError::Stale);
        }
        if count == 2 {
            let oldest = oldest.ok_or(GrantError::Bounds)?;
            self.values.remove(&(name.account, oldest));
        }
        match self.values.insert((name.account, name.generation), Entry { credential, lapses }) {
            Ok(None) => Ok(()),
            Ok(Some(_)) => Err(GrantError::Stale),
            Err(_) => Err(GrantError::Full),
        }
    }

    /// Clone only the requested still-valid value for the connection's Start.
    /// A missing or lapsed grant has no lower effect.
    pub fn read(&self, name: GrantName, now: Time) -> Result<Credential, GrantError> {
        let entry = self.values.get(&(name.account, name.generation)).ok_or(GrantError::Missing)?;
        if entry.lapses <= now {
            return Err(GrantError::Lapsed);
        }
        Ok(Credential {
            access_token: bytes::copy_of(&entry.credential.access_token),
            account_id: bytes::copy_of(&entry.credential.account_id),
        })
    }

    /// Worst retained table and secret payload ownership for configured caps.
    #[must_use]
    pub fn worst_case(accounts: u32, value_bytes: u32) -> Option<u64> {
        let capacity = accounts.checked_mul(2)?;
        Map::<(u32, u64), Entry>::worst_case(capacity)?
            .checked_add(u64::from(capacity).checked_mul(u64::from(value_bytes))?)
    }
}
