//! Bounded guide prefixes and executable probes (protocol/agent.md, section 2).
//! The open descriptor is kept only until its close terminal; the component
//! knows no file content before the file layer reports it.

use alloc::boxed::Box;

use skein_io::{file, kernel};
use skein_lib::{Queue, Time, Token};
use smith_domain::run;

use crate::boundary::{Below, ToDomain};

/// One read or probe waiting for the next file-layer terminal.
#[derive(Debug)]
pub(crate) enum Guide {
    /// An open has been requested for a guide prefix.
    OpeningRead { max: u32, deadline: Time },
    /// An open has been requested for executable metadata.
    OpeningProbe { deadline: Time },
    /// A bounded prefix is being read.
    Reading { file: Token, len: u64, max: u32 },
    /// Metadata is being read.
    Probing { file: Token },
    /// The descriptor is closing before the domain terminal.
    Closing { terminal: ToDomain },
}

pub(crate) enum Step {
    Continue(Guide),
    Done(ToDomain),
}

impl Guide {
    #[expect(clippy::wildcard_enum_match_arm, reason = "skein event and kernel error vocabularies are external")]
    pub(crate) fn event(self, owner: Token, event: file::Event, below: &mut Queue<Below>) -> Step {
        match self {
            Guide::OpeningRead { max, deadline } => match event {
                file::Event::Opened { file, len, .. } => {
                    if max == 0 {
                        close(
                            owner,
                            file,
                            ToDomain::Read { owner, read: run::Read::Text { text: Box::new([]), whole: len == 0 } },
                            below,
                        )
                    } else {
                        let requested = u32::try_from(len.min(u64::from(max))).expect("bounded read");
                        below.push(Below::File {
                            request: file::Request::ReadAt { owner, file, offset: 0, max: requested },
                            deadline,
                        });
                        Step::Continue(Guide::Reading { file, len, max })
                    }
                }
                file::Event::Failed { error, .. } => {
                    let read = match error {
                        kernel::Error::NotFound | kernel::Error::NotAFile | kernel::Error::NotADirectory => {
                            run::Read::Missing
                        }
                        _ => run::Read::Failed,
                    };
                    Step::Done(ToDomain::Read { owner, read })
                }
                file::Event::Cancelled { .. } => Step::Done(ToDomain::Read { owner, read: run::Read::Failed }),
                _ => unreachable!("open read terminal"),
            },
            Guide::OpeningProbe { deadline } => match event {
                file::Event::Opened { file, .. } => {
                    below.push(Below::File { request: file::Request::Stat { owner, file }, deadline });
                    Step::Continue(Guide::Probing { file })
                }
                file::Event::Failed { .. } | file::Event::Cancelled { .. } => {
                    Step::Done(ToDomain::Probed { owner, executable: false })
                }
                _ => unreachable!("probe open terminal"),
            },
            Guide::Reading { file, len, max } => {
                let read = match event {
                    file::Event::Read { bytes, .. } => text(bytes, len, max),
                    file::Event::Failed { .. } | file::Event::Cancelled { .. } => run::Read::Failed,
                    _ => unreachable!("read terminal"),
                };
                close(owner, file, ToDomain::Read { owner, read }, below)
            }
            Guide::Probing { file } => {
                let executable = match event {
                    file::Event::Stated { stat, .. } => stat.kind == kernel::Kind::File && stat.mode & 0o111 != 0,
                    file::Event::Failed { .. } | file::Event::Cancelled { .. } => false,
                    _ => unreachable!("stat terminal"),
                };
                close(owner, file, ToDomain::Probed { owner, executable }, below)
            }
            Guide::Closing { terminal } => {
                match event {
                    file::Event::Closed { .. } | file::Event::Failed { .. } | file::Event::Cancelled { .. } => {}
                    _ => unreachable!("close terminal"),
                }
                Step::Done(terminal)
            }
        }
    }
}

fn close(owner: Token, file: Token, terminal: ToDomain, below: &mut Queue<Below>) -> Step {
    below.push(Below::File { request: file::Request::Close { owner, file }, deadline: Time::from_nanos(u64::MAX) });
    Step::Continue(Guide::Closing { terminal })
}

#[expect(clippy::disallowed_methods, reason = "the protocol machine validates guide UTF-8")]
fn text(bytes: Box<[u8]>, len: u64, max: u32) -> run::Read {
    match core::str::from_utf8(&bytes) {
        Ok(_) => run::Read::Text { whole: len <= u64::from(max), text: bytes },
        Err(error) => {
            if error.error_len().is_some() || len <= u64::from(max) {
                run::Read::NotText
            } else {
                run::Read::Text {
                    text: Box::from(bytes.get(..error.valid_up_to()).expect("UTF-8 prefix is in bounds")),
                    whole: false,
                }
            }
        }
    }
}
