//! Workspace outcomes into bounded text for a live or resumed LLM prompt.
//! The domain chooses retained content; this module chooses only its words.
//! It holds no state across calls and never decides whether an effect ran.
//! Contract: protocol/llm.md, section 5; protocol/transcript.md, section 5;
//! domain/tools.md, section 4.

use alloc::boxed::Box;

use skein_lib::{Decimal, Writer};
use skein_llm::Error;
use smith_domain::tools::{self, Outcome};

/// Room for a newline, brackets, a u64 byte count and the cut's words.
/// Derived result bounds always hold this marker (protocol/limits.md, section 6).
pub const CUT_MARKER_BYTES: u32 = 37;

/// Render workspace feedback, cutting its prefix with the exact omitted bytes.
/// The caller derives a bound large enough for the marker before admitting a
/// prompt. Cutting preserves the outcome's error flag and every UTF-8 boundary.
pub fn render_outcome(outcome: &Outcome, maximum: u32) -> Result<(Box<[u8]>, bool), Error> {
    assert!(maximum >= CUT_MARKER_BYTES, "derived render bound holds the cut marker");
    let mut measure = Sink { length: 0, kept: 0, maximum: u64::MAX, cut: false, writer: None };
    let error = render(outcome, &mut measure)?;
    let prefix_cap = if measure.length > u64::from(maximum) {
        u64::from(maximum.checked_sub(CUT_MARKER_BYTES).expect("marker room checked"))
    } else {
        u64::from(maximum)
    };
    let mut prefix = Sink { length: 0, kept: 0, maximum: prefix_cap, cut: false, writer: None };
    let again = render(outcome, &mut prefix)?;
    assert_eq!(error, again, "render classification is stable");
    let omitted = measure.length.checked_sub(prefix.kept).expect("prefix within full rendering");
    let marker_length = if omitted > 0 {
        u64::try_from(Decimal::of(omitted).as_bytes().len())
            .or(Err(Error::Limit))?
            .checked_add(17)
            .ok_or(Error::Limit)?
    } else {
        0
    };
    let length = usize::try_from(prefix.kept.checked_add(marker_length).ok_or(Error::Limit)?).or(Err(Error::Limit))?;
    let mut output = Sink { length: 0, kept: 0, maximum: prefix_cap, cut: false, writer: Some(Writer::new(length)) };
    let rendered = render(outcome, &mut output)?;
    assert_eq!(error, rendered, "render classification is stable");
    assert_eq!(output.kept, prefix.kept, "render prefix is stable");
    let mut writer = output.writer.expect("writing pass owns its measured output");
    if omitted > 0 {
        writer.put(b"\n[").expect("measured marker room");
        writer.put(Decimal::of(omitted).as_bytes()).expect("measured byte-count room");
        writer.put(b" bytes omitted]").expect("measured marker room");
    }
    Ok((writer.finish(), error))
}

struct Sink {
    length: u64,
    kept: u64,
    maximum: u64,
    cut: bool,
    writer: Option<Writer>,
}

impl Sink {
    fn put(&mut self, text: &[u8]) -> Result<(), Error> {
        self.length = self.length.checked_add(u64::try_from(text.len()).or(Err(Error::Limit))?).ok_or(Error::Limit)?;
        if self.cut {
            return Ok(());
        }
        let available = self.maximum.checked_sub(self.kept).expect("kept prefix within cap");
        let wanted = usize::try_from(available).unwrap_or(usize::MAX).min(text.len());
        let mut boundary = wanted;
        // At most three continuation bytes precede a UTF-8 boundary. No byte
        // beyond the cap is copied, including when a multi-byte scalar straddles it.
        for _byte in 0_u8..3 {
            match text.get(boundary) {
                Some(byte) if byte & 0xc0 == 0x80 && boundary > 0 => {
                    boundary = boundary.checked_sub(1).expect("positive boundary");
                }
                Some(_) | None => break,
            }
        }
        let retained = text.get(..boundary).expect("boundary within text");
        self.kept = self.kept.checked_add(u64::try_from(retained.len()).or(Err(Error::Limit))?).ok_or(Error::Limit)?;
        self.cut = boundary < text.len();
        match &mut self.writer {
            Some(writer) => writer.put(retained).or(Err(Error::Limit)),
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
fn render(outcome: &Outcome, output: &mut Sink) -> Result<bool, Error> {
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

fn read(content: &[u8], skipped: u32, lines: u32, total: u32, cut: bool, output: &mut Sink) -> Result<(), Error> {
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

/// Cut domain-written text without changing its UTF-8 or allocating its full size.
pub(crate) fn cut_text(text: Box<[u8]>, maximum: u32) -> Box<[u8]> {
    assert!(maximum >= CUT_MARKER_BYTES, "derived render bound holds the cut marker");
    if text.len() <= usize::try_from(maximum).expect("u32 fits usize") {
        return text;
    }
    let cap = u64::from(maximum.checked_sub(CUT_MARKER_BYTES).expect("marker room checked"));
    let mut measure = Sink { length: 0, kept: 0, maximum: cap, cut: false, writer: None };
    measure.put(&text).expect("owned text length fits u64");
    let omitted = measure.length.checked_sub(measure.kept).expect("prefix within full text");
    let length = usize::try_from(measure.kept)
        .expect("bounded prefix fits usize")
        .checked_add(Decimal::of(omitted).as_bytes().len())
        .expect("marker within derived cap")
        .checked_add(17)
        .expect("marker within derived cap");
    let mut output = Sink { length: 0, kept: 0, maximum: cap, cut: false, writer: Some(Writer::new(length)) };
    output.put(&text).expect("measured prefix room");
    let mut writer = output.writer.expect("writing pass owns its measured output");
    writer.put(b"\n[").expect("measured marker room");
    writer.put(Decimal::of(omitted).as_bytes()).expect("measured byte-count room");
    writer.put(b" bytes omitted]").expect("measured marker room");
    writer.finish()
}
