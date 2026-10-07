//! Workspace outcomes into bounded text for a live or resumed LLM prompt.
//! The domain chooses retained content; this module chooses only its words.
//! It holds no state across calls and never decides whether an effect ran.
//! Contract: protocol/llm.md, section 5; protocol/transcript.md, section 5;
//! domain/tools.md, section 4.

use alloc::boxed::Box;

use skein_lib::{Decimal, Writer};
use skein_llm::Error;
use smith_domain::tools::{self, Outcome};

/// Render a typed workspace outcome for either live feedback or saved history.
/// The boolean is the dialect's tool-result error flag. A result exceeding the
/// configured text bound is refused before it can reach the provider.
pub fn render_outcome(outcome: &Outcome, maximum: u32) -> Result<(Box<[u8]>, bool), Error> {
    let mut measure = Sink { length: 0, writer: None };
    let error = render(outcome, &mut measure)?;
    if measure.length > u64::from(maximum) {
        return Err(Error::Limit);
    }
    let length = usize::try_from(measure.length).or(Err(Error::Limit))?;
    let mut writer = Writer::new(length);
    let mut output = Sink { length: 0, writer: Some(&mut writer) };
    let again = render(outcome, &mut output)?;
    assert_eq!(error, again, "render classification is stable");
    assert_eq!(output.length, measure.length, "render measurement is stable");
    Ok((writer.finish(), error))
}

struct Sink<'a> {
    length: u64,
    writer: Option<&'a mut Writer>,
}

impl Sink<'_> {
    fn put(&mut self, text: &[u8]) -> Result<(), Error> {
        self.length = self.length.checked_add(u64::try_from(text.len()).or(Err(Error::Limit))?).ok_or(Error::Limit)?;
        match &mut self.writer {
            Some(writer) => writer.put(text).or(Err(Error::Limit)),
            None => Ok(()),
        }
    }

    fn number(&mut self, number: u64) -> Result<(), Error> {
        self.put(Decimal::of(number).as_bytes())
    }

    fn text(&mut self, bytes: &[u8]) -> Result<(), Error> {
        for byte in bytes {
            match *byte {
                b'\n' | b'\t' | 32..=126 => self.put(&[*byte])?,
                _ => {
                    let high = hex(byte >> 4_u8);
                    let low = hex(byte & 15_u8);
                    self.put(&[b'\\', b'x', high, low])?;
                }
            }
        }
        Ok(())
    }
}

fn hex(nibble: u8) -> u8 {
    *b"0123456789abcdef".get(usize::from(nibble)).expect("four-bit byte")
}

#[expect(clippy::too_many_lines, reason = "one exhaustive rendering cell per outcome variant")]
fn render(outcome: &Outcome, output: &mut Sink<'_>) -> Result<bool, Error> {
    let error = match outcome {
        Outcome::Read { content, skipped, lines, total, cut } => {
            read(content, *skipped, *lines, *total, *cut, output)?;
            false
        }
        Outcome::Listed { entries, more } => {
            for entry in entries {
                output.text(entry.name.as_bytes())?;
                output.put(b"\t")?;
                let kind = match entry.kind {
                    tools::Kind::File => b"file".as_slice(),
                    tools::Kind::Directory => b"directory",
                    tools::Kind::Link => b"link",
                    tools::Kind::Other => b"other",
                };
                output.put(kind)?;
                output.put(b"\n")?;
            }
            if *more > 0 {
                output.put(b"... ")?;
                output.number(*more)?;
                output.put(b" more entries")?;
            }
            false
        }
        Outcome::Found { hits, more, timed_out } => {
            for hit in hits {
                output.text(&hit.path)?;
                output.put(b":")?;
                output.number(u64::from(hit.line))?;
                output.put(b": ")?;
                output.text(&hit.text)?;
                output.put(b"\n")?;
            }
            if *more > 0 {
                output.put(b"... ")?;
                output.number(*more)?;
                output.put(b" more matches\n")?;
            }
            if *timed_out {
                output.put(b"search timed out")?;
            }
            *timed_out
        }
        Outcome::Written { created } => {
            if *created {
                output.put(b"file created")?;
            } else {
                output.put(b"file written")?;
            }
            false
        }
        Outcome::Edited { replaced } => {
            output.put(b"replaced ")?;
            output.number(u64::from(*replaced))?;
            output.put(b" occurrence(s)")?;
            false
        }
        Outcome::Exited { exit, head, tail, dropped } => {
            let failure = match exit {
                tools::Exit::Code { code } => {
                    output.put(b"exit code ")?;
                    output.number(u64::from(*code))?;
                    *code != 0
                }
                tools::Exit::Signal { signal } => {
                    output.put(b"signal ")?;
                    output.number(u64::from(*signal))?;
                    true
                }
                tools::Exit::TimedOut => {
                    output.put(b"command timed out")?;
                    true
                }
            };
            if !head.is_empty() {
                output.put(b"\n")?;
                output.text(head)?;
            }
            if *dropped > 0 {
                output.put(b"\n... ")?;
                output.number(*dropped)?;
                output.put(b" bytes dropped ...")?;
            }
            if !tail.is_empty() {
                output.put(b"\n")?;
                output.text(tail)?;
            }
            failure
        }
        Outcome::NotGranted => {
            output.put(b"tool not granted")?;
            true
        }
        Outcome::Outside => {
            output.put(b"path is outside the workspace")?;
            true
        }
        Outcome::ReadOnly => {
            output.put(b"path is read-only")?;
            true
        }
        Outcome::TooLong => {
            output.put(b"path is too long")?;
            true
        }
        Outcome::NotFound => {
            output.put(b"path not found")?;
            true
        }
        Outcome::NotFile => {
            output.put(b"path is not a file")?;
            true
        }
        Outcome::Linked => {
            output.put(b"write path crosses a symbolic link")?;
            true
        }
        Outcome::Protected => {
            output.put(b"git directory is protected")?;
            true
        }
        Outcome::NotDirectory => {
            output.put(b"path is not a directory")?;
            true
        }
        Outcome::TooLarge { size } => {
            output.put(b"file or content too large: ")?;
            output.number(*size)?;
            output.put(b" bytes")?;
            true
        }
        Outcome::NotRead => {
            output.put(b"read the file before changing it")?;
            true
        }
        Outcome::Stale => {
            output.put(b"file changed; read it again")?;
            true
        }
        Outcome::NoMatch => {
            output.put(b"text to replace was not found")?;
            true
        }
        Outcome::Ambiguous { count, lines } => {
            output.put(b"text matches ")?;
            output.number(u64::from(*count))?;
            output.put(b" times, starting on lines ")?;
            for (index, line) in lines.iter().enumerate() {
                if index > 0 {
                    output.put(b", ")?;
                }
                output.number(u64::from(*line))?;
            }
            true
        }
        Outcome::Unchanged => {
            output.put(b"replacement leaves the file unchanged")?;
            true
        }
        Outcome::Failed { fault } => {
            let text = match fault {
                tools::Fault::Denied => b"operation failed: permission denied".as_slice(),
                tools::Fault::NoSpace => b"operation failed: no space",
                tools::Fault::Other => b"operation failed: io error",
            };
            output.put(text)?;
            true
        }
        Outcome::TimedOut => {
            output.put(b"operation timed out")?;
            true
        }
        Outcome::Cancelled => {
            output.put(b"operation cancelled")?;
            true
        }
        Outcome::Busy => {
            output.put(b"tool is busy")?;
            true
        }
        Outcome::NulByte => {
            output.put(b"input contains a NUL byte")?;
            true
        }
    };
    Ok(error)
}

fn read(content: &[u8], skipped: u32, lines: u32, total: u32, cut: bool, output: &mut Sink<'_>) -> Result<(), Error> {
    let mut start = 0_usize;
    let mut index = 0_u64;
    for (position, byte) in content.iter().enumerate() {
        if *byte == b'\n' {
            output.number(
                u64::from(skipped).checked_add(index).ok_or(Error::Limit)?.checked_add(1).ok_or(Error::Limit)?,
            )?;
            output.put(b": ")?;
            output.text(content.get(start..position).ok_or(Error::Limit)?)?;
            output.put(b"\n")?;
            start = position.checked_add(1).ok_or(Error::Limit)?;
            index = index.checked_add(1).ok_or(Error::Limit)?;
        }
    }
    if start < content.len() {
        output
            .number(u64::from(skipped).checked_add(index).ok_or(Error::Limit)?.checked_add(1).ok_or(Error::Limit)?)?;
        output.put(b": ")?;
        output.text(content.get(start..).ok_or(Error::Limit)?)?;
        output.put(b"\n")?;
    }
    output.put(b"(")?;
    output.number(u64::from(lines))?;
    output.put(b" of ")?;
    output.number(u64::from(total))?;
    output.put(b" lines; skipped ")?;
    output.number(u64::from(skipped))?;
    output.put(b")")?;
    if cut {
        output.put(b" [line cut]")?;
    }
    Ok(())
}
