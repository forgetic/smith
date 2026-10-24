//! Driver terminals retain redacted state for hosts (protocol/hosts.md, 5.4).
use super::{Accounts, CredentialRequest, Origin, Settled, account_form, claim_identifier, oauth};
use skein_lib::{Queue, Time, Wall};
use smith_local_domain as local;
impl Accounts {
    pub(super) fn emit(&mut self, now: Time, wall: Wall, out: &mut Queue<CredentialRequest>) {
        if let Some((account, reason)) = self.failed.take() {
            out.push(CredentialRequest::Failed { event: local::Event::NoCredential { account, reason } });
        }
        for _ in 0..3_u32 {
            match self.out.pop() {
                Some(oauth::Event::Granted { account: index, token, generation, valid }) => {
                    let account = self.entries.get_mut(index).expect("account index");
                    account.wanted = false;
                    account.loading = false;
                    if !claim_identifier(account, &token, wall, &self.limits.client.document) {
                        out.push(CredentialRequest::Failed {
                            event: local::Event::NoCredential {
                                account: account.number,
                                reason: local::CredentialFailure::Missing,
                            },
                        });
                        self.down(oauth::Request::Release { account: index }, now, wall);
                        continue;
                    }
                    out.push(CredentialRequest::Ready {
                        event: local::Event::Credential {
                            form: account_form(&account.identifier),
                            grant: smith_domain::Grant {
                                name: smith_domain::GrantName { account: account.number, generation },
                                valid,
                                renewable: true,
                            },
                        },
                        value: token,
                    });
                }
                Some(oauth::Event::Expiring { account, .. }) => {
                    let account = self.entries.get(account).expect("account index");
                    out.push(CredentialRequest::Notice {
                        account: account.number,
                        reason: local::CredentialFailure::BorrowedExpiring { at: account.expires },
                    });
                }
                Some(oauth::Event::Failed { account: index, ends, failure }) => {
                    self.failed_account(index, ends, failure, out);
                }
                Some(oauth::Event::Refused { account: index, asked, why }) => {
                    if asked == oauth::Asked::SignOut && why == oauth::Refusal::Busy {
                        let account = self.entries.get_mut(index).expect("account index");
                        account.loading = false;
                        account.pending = Some(oauth::Request::SignOut { account: index });
                        account.startup = true;
                        continue;
                    }
                    self.remember(
                        index,
                        if why == oauth::Refusal::NoRecord && asked == oauth::Asked::SignOut {
                            Settled::SignedOut { revoked: true }
                        } else {
                            Settled::Failed { ends: oauth::Ends::SignIn, failure: oauth::Failure::Busy }
                        },
                    );
                    let account = self.entries.get_mut(index).expect("account index");
                    account.loading = false;
                    account.wanted = false;
                    out.push(CredentialRequest::Failed {
                        event: local::Event::NoCredential {
                            account: account.number,
                            reason: local::CredentialFailure::Missing,
                        },
                    });
                }
                Some(oauth::Event::Visit { url, .. }) => {
                    out.push(CredentialRequest::Visit { url });
                }
                Some(oauth::Event::SignedIn { account: index, missing, subject, .. }) => {
                    self.remember(index, Settled::SignedIn { missing, subject });
                    let account = self.entries.get_mut(index).expect("account index");
                    account.loading = false;
                    match &mut account.source {
                        Origin::SignIn { signing, .. } => *signing = false,
                        Origin::Env { .. } => unreachable!("sign-in terminal"),
                    }
                    out.push(CredentialRequest::SignedIn { account: account.number });
                }
                Some(oauth::Event::Closed) => self.closed = true,
                Some(oauth::Event::Lock { account }) => self.take_lock(account, now),
                Some(oauth::Event::Unlock { account }) => self.release_lock(account),
                Some(oauth::Event::SignedOut { account: index, revoked }) => {
                    self.entries.get_mut(index).expect("account index").loading = false;
                    self.remember(index, Settled::SignedOut { revoked });
                }
                Some(oauth::Event::Removed { .. }) => {}
                Some(oauth::Event::Keep { .. }) => {
                    unreachable!("private keepers own their durable records")
                }
                Some(oauth::Event::Taken { .. }) => unreachable!("smith hands in no redirected"),
                None => break,
            }
        }
    }

    fn failed_account(
        &mut self,
        index: u32,
        ends: oauth::Ends,
        failure: oauth::Failure,
        out: &mut Queue<CredentialRequest>,
    ) {
        let account = self.entries.get_mut(index).expect("account index");
        let failure = account.subject_failure.take().unwrap_or(failure);
        self.remember(index, Settled::Failed { ends, failure });
        let account = self.entries.get_mut(index).expect("account index");
        account.loading = false;
        match &mut account.source {
            Origin::SignIn { interactive, sign_in, signing, .. } => {
                *signing = false;
                if *interactive
                    && self.closing.is_none()
                    && ends == oauth::Ends::Grant
                    && failure == oauth::Failure::Expired
                {
                    *sign_in = true;
                    account.startup = true;
                    return;
                }
            }
            Origin::Env { .. } => {}
        }
        out.push(CredentialRequest::Failed {
            event: local::Event::NoCredential { account: account.number, reason: local::CredentialFailure::Missing },
        });
        account.wanted = false;
    }
}
