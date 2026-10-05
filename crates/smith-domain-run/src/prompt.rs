//! What a run tells its LLMs (domain/run.md, section 14): the system text, which is
//! the brief (the charter's for main, the asker's for a sub-agent), then what
//! the run found in its checkout (each repository's `AGENTS.md`), then the
//! sections on the run's own mechanics (its tools, its checkout, its
//! sub-agents, and how to finish, or for a sub-agent how to answer); and the
//! nudges. The brief and the guides go in as
//! they came; the rest is rendered here from constant fragments and the
//! charter's data, its labels verbatim.
//!
//! A text is rendered twice: once to measure it, then into a [`Writer`] of
//! exactly that length (programming-model.md, section 8).

use alloc::boxed::Box;

use skein_lib::Writer;

use crate::boundary::Stop;
use crate::charter::{Charter, Families, Llm, Repository, Tools};
use crate::outcome::{FieldRule, OutcomeSpec, TextSpec, VerdictRule};
use crate::prepare::{self, Found, Guide};

/// The first user message of a main conversation.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) const BEGIN: &[u8] = b"Begin the work your brief describes.";

/// The system text of a run's main conversation, given what the run found in
/// its checkout.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn system(charter: &Charter, found: &Found) -> Box<[u8]> {
    let families = Families::of(&charter.grants);
    let mut measured = Text::measuring();
    render_system(&mut measured, charter, found, &charter.brief, families, true);
    let mut text = measured.writing();
    render_system(&mut text, charter, found, &charter.brief, families, true);
    text.finish()
}

/// The system text of a sub-agent asked for with `brief` and `families`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn child(charter: &Charter, found: &Found, brief: &[u8], families: Families) -> Box<[u8]> {
    let mut measured = Text::measuring();
    render_system(&mut measured, charter, found, brief, families, false);
    let mut text = measured.writing();
    render_system(&mut text, charter, found, brief, families, false);
    text.finish()
}

/// What a run says to an LLM that stopped for `stop` without finishing, in its
/// nudge numbered `nudge` of `nudges`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn nudge(stop: Stop, nudge: u32, nudges: u32) -> Box<[u8]> {
    let mut measured = Text::measuring();
    render_nudge(&mut measured, stop, nudge, nudges);
    let mut text = measured.writing();
    render_nudge(&mut text, stop, nudge, nudges);
    text.finish()
}

/// The system text of main, or of a sub-agent: on `brief`, with `families`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn render_system(text: &mut Text, charter: &Charter, found: &Found, brief: &[u8], families: Families, main: bool) {
    if !brief.is_empty() {
        text.put(brief);
        end_paragraph(text, brief);
    }
    let repositories = &charter.checkout.repositories;
    for guide in &found.guides {
        render_guide(text, repositories, guide);
    }
    render_tools(text, families.tools);
    text.put(b"\n");
    render_checkout(text, repositories, found.checks.as_slice());
    text.put(b"\n");
    if families.agents {
        render_agents(text, &charter.models);
        text.put(b"\n");
    }
    if main {
        render_finishing(text, &charter.outcome, !found.checks.is_empty());
    } else {
        text.put(b"## Answering\n\nWhen you are done, end your turn with your answer: your last message goes, as it ");
        text.put(b"is, to the LLM that asked for you, and you are done.\n");
    }
}

fn render_agents(text: &mut Text, models: &[Llm]) {
    text.put(b"## Sub-agents\n\n");
    text.put(
        b"You can ask for a sub-agent: an LLM of its own, working on a brief you write, with tools no wider than ",
    );
    text.put(b"yours and a share of the budget. Its last message comes back to you as the result. It runs on the ");
    if models.is_empty() {
        text.put(b"run's main LLM.\n");
        return;
    }
    text.put(b"run's main LLM unless you name one of these: ");
    let mut rest = models.len();
    for llm in models {
        text.put(b"`");
        text.put(&llm.model);
        text.put(b"`");
        rest = rest.saturating_sub(1);
        match rest {
            0 => {}
            1 => text.put(b" or "),
            _ => text.put(b", "),
        }
    }
    text.put(b".\n");
}

/// One blank line after a text that came as it is, whether or not it ends
/// its last line.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn end_paragraph(text: &mut Text, came: &[u8]) {
    if !came.ends_with(b"\n") {
        text.put(b"\n");
    }
    text.put(b"\n");
}

fn render_guide(text: &mut Text, repositories: &[Repository], guide: &Guide) {
    text.put(b"## ");
    text.put(prepare::GUIDE);
    text.put(b" in `");
    text.put(&name(repositories, guide.repository).name);
    text.put(b"`\n\n");
    if !guide.text.is_empty() {
        text.put(&guide.text);
        end_paragraph(text, &guide.text);
    }
    if !guide.whole {
        text.put(b"(The file goes on: read the rest with your tools.)\n\n");
    }
}

fn render_tools(text: &mut Text, tools: Tools) {
    let Tools { inspect, modify, shell } = tools;
    text.put(b"## Tools\n\n");
    if inspect {
        text.put(b"You can read, list and search the files in the checkout.\n");
    }
    if modify {
        text.put(b"You can write and edit files in its writable repositories.\n");
    }
    if shell {
        text.put(b"You can run shell commands.\n");
    }
    if !inspect && !modify && !shell {
        text.put(b"You have no tools that act on the checkout.\n");
    }
}

fn render_checkout(text: &mut Text, repositories: &[Repository], checks: &[u32]) {
    text.put(b"## Checkout\n\n");
    if repositories.is_empty() {
        text.put(b"There is no checkout.\n");
    }
    let mut index: u32 = 0;
    for Repository { name, root: _, writable } in repositories {
        text.put(b"- `");
        text.put(name);
        text.put(if *writable { b"`, which you may change" } else { b"`, which you may only read" });
        if checks.contains(&index) {
            text.put(b", with checks (`");
            text.put(prepare::CHECKS);
            text.put(b"`)");
        }
        text.put(b"\n");
        index = index.saturating_add(1);
    }
}

fn render_finishing(text: &mut Text, spec: &OutcomeSpec, checks: bool) {
    text.put(b"## Finishing\n\n");
    text.put(b"When the work is done, call `finish` with its result. A result outside the host's contract returns ");
    text.put(b"typed feedback; fix it and call `finish` again. Stopping without `finish` does not finish the run.\n");
    if let Some(rule) = &spec.change {
        text.put(b"\nChange: the workspace changes, with these host-required result fields:\n");
        render_fields(text, &rule.fields);
        if rule.checks && checks {
            text.put(b"Checks run before the host receives the change. A failed check returns its output so you can fix it.\n");
        }
        text.put(b"The host pushes the checked state. A moved target ends this run; other refusals are feedback.\n");
    }
    if let Some(rule) = &spec.report {
        render_text_rule(text, b"Report", rule);
    }
    if let Some(rule) = &spec.failure {
        render_text_rule(text, b"Declared failure", rule);
    }
    if !spec.verdicts.is_empty() {
        text.put(b"\nVerdict: one exact host label from this closed list:\n");
        for rule in &spec.verdicts {
            render_verdict(text, rule);
        }
    }
    text.put(b"Extra fields are allowed within the aggregate result byte limit; no field name may repeat.\n");
}

fn render_text_rule(text: &mut Text, form: &[u8], rule: &TextSpec) {
    text.put(b"\n");
    text.put(form);
    text.put(b": text from ");
    text.put_decimal(rule.min);
    text.put(b" through ");
    text.put_decimal(rule.max);
    text.put(b" bytes, with these host-required fields:\n");
    render_fields(text, &rule.fields);
}

fn render_fields(text: &mut Text, fields: &[FieldRule]) {
    if fields.is_empty() {
        text.put(b"(none)\n");
    }
    for field in fields {
        text.put(b"- `");
        text.put(&field.name);
        text.put(b"`: nonempty, at most ");
        text.put_decimal(field.max);
        text.put(b" bytes\n");
    }
}

fn name(repositories: &[Repository], index: u32) -> &Repository {
    let index = usize::try_from(index).expect("a u32 fits in a usize");
    repositories.get(index).expect("a guide is of a repository of the checkout")
}

/// A verdict and its contract, as one item of a list.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn render_verdict(text: &mut Text, rule: &VerdictRule) {
    text.put(b"- `");
    text.put(&rule.name);
    text.put(b"`: text at most ");
    text.put_decimal(rule.text_max);
    text.put(b" bytes; required result fields:\n");
    render_fields(text, &rule.fields);
    text.put(b"Items: ");
    text.put_decimal(rule.items.min);
    text.put(b" to ");
    text.put_decimal(rule.items.max);
    text.put(b"; allowed kinds and their required fields:\n");
    for kind in &rule.items.kinds {
        text.put(b"`");
        text.put(&kind.kind);
        text.put(b"`:\n");
        render_fields(text, &kind.fields);
    }
}

fn render_nudge(text: &mut Text, stop: Stop, nudge: u32, nudges: u32) {
    text.put(match stop {
        Stop::EndTurn => b"You stopped without calling `finish`, so the work is not done.",
        Stop::MaxTokens => b"Your answer was cut off: it ran out of tokens.",
        Stop::Refusal => b"You declined to go on.",
        Stop::NoCalls => b"You said you would call a tool, and named none.",
    });
    text.put(b" Carry on with the work, and call `finish` when it is done. This is reminder ");
    text.put_decimal(nudge);
    text.put(b" of ");
    text.put_decimal(nudges);
    text.put(b": if you stop again after the last one, the run ends unfinished.");
}

/// A text being rendered: measured first, then written into a box of exactly
/// the length measured.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
struct Text {
    /// The bytes put so far.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    len: usize,
    /// Where they go, once the text has been measured.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    writer: Option<Writer>,
}

impl Text {
    fn measuring() -> Text {
        Text { len: 0, writer: None }
    }

    /// A text to write what this one measured.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    fn writing(self) -> Text {
        Text { len: 0, writer: Some(Writer::new(self.len)) }
    }

    fn put(&mut self, bytes: &[u8]) {
        self.len = self.len.saturating_add(bytes.len());
        if let Some(writer) = &mut self.writer {
            writer.put(bytes).expect("a text is written as it was measured");
        }
    }

    /// `n` in decimal digits.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    fn put_decimal(&mut self, n: u32) {
        let mut digits = [b'0'; 10];
        let mut rest = n;
        // The first digit to put: the leftmost that is not a leading zero.
        let mut first: usize = 9;
        for (index, digit) in digits.iter_mut().enumerate().rev() {
            let value = u8::try_from(rest.checked_rem(10).unwrap_or(0)).expect("a digit fits in a byte");
            *digit = b'0'.saturating_add(value);
            if value != 0 {
                first = index;
            }
            rest = rest.checked_div(10).unwrap_or(0);
        }
        self.put(digits.get(first..).unwrap_or_default());
    }

    fn finish(self) -> Box<[u8]> {
        self.writer.expect("a text is finished once written").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{Text, child, nudge, system};
    use crate::boundary::Stop;
    use crate::charter::{Charter, Checkout, Families, Grants, Tools};
    use crate::outcome::{ChangeSpec, FieldRule, ItemRule, ItemSpec, OutcomeSpec, TextSpec, VerdictRule};
    use crate::prepare::{Found, Guide};
    use crate::tests::{bytes, charter};
    use alloc::boxed::Box;

    #[expect(clippy::disallowed_methods, reason = "a test reads the exact fixture text it checks")]
    fn text(bytes: &[u8]) -> &str {
        core::str::from_utf8(bytes).expect("fixture text")
    }

    fn found() -> Found {
        let mut found = Found::with_capacity(2);
        found
            .guides
            .push(Guide { repository: 0, text: bytes(b"Run `make test` before you finish."), whole: false })
            .expect("room");
        found.checks.push(1).expect("room");
        found
    }

    fn fields(name: &[u8], max: u32) -> Box<[FieldRule]> {
        Box::new([FieldRule { name: bytes(name), max }])
    }

    #[test]
    fn host_names_and_individual_caps_are_rendered_without_builtin_change_fields() {
        let charter = Charter {
            outcome: OutcomeSpec {
                change: Some(ChangeSpec { checks: true, fields: fields(b"ticket", 17) }),
                report: Some(TextSpec { min: 0, max: 32, fields: fields(b"source", 9) }),
                failure: Some(TextSpec { min: 0, max: 24, fields: Box::new([]) }),
                verdicts: Box::new([VerdictRule {
                    name: bytes(b"triaged"),
                    text_max: 11,
                    fields: fields(b"owner", 6),
                    items: ItemSpec {
                        min: 1,
                        max: 2,
                        kinds: Box::new([
                            ItemRule { kind: bytes(b"risk"), fields: fields(b"severity", 4) },
                            ItemRule { kind: bytes(b"lead"), fields: fields(b"url", 19) },
                        ]),
                    },
                }]),
            },
            ..charter()
        };
        let rendered = system(&charter, &found());
        let rendered = text(&rendered);
        let brief = rendered.find("Review the change.").expect("brief");
        let guide = rendered.find("## AGENTS.md").expect("guide");
        let finish = rendered.find("## Finishing").expect("finish");
        assert!(brief < guide && guide < finish, "brief, guides then mechanics");
        for fragment in [
            "`ticket`: nonempty, at most 17 bytes",
            "Report: text from 0 through 32 bytes",
            "`source`: nonempty, at most 9 bytes",
            "Declared failure: text from 0 through 24 bytes",
            "`triaged`: text at most 11 bytes",
            "`owner`: nonempty, at most 6 bytes",
            "Items: 1 to 2",
            "`risk`:\n- `severity`",
            "`lead`:\n- `url`",
        ] {
            assert!(rendered.contains(fragment), "missing {fragment}: {rendered}");
        }
        assert!(
            !rendered.contains("pull request") && !rendered.contains("`title`") && !rendered.contains("`body`"),
            "host names only"
        );
    }

    #[test]
    fn absent_workspace_and_empty_field_rules_are_said_plainly() {
        let charter = Charter {
            checkout: Checkout { repositories: Box::new([]) },
            grants: Grants { tools: Tools { inspect: false, modify: false, shell: false }, ..charter().grants },
            outcome: OutcomeSpec {
                change: None,
                verdicts: Box::new([]),
                report: Some(TextSpec { min: 0, max: 16, fields: Box::new([]) }),
                failure: None,
            },
            ..charter()
        };
        let rendered = system(&charter, &Found::with_capacity(0));
        let rendered = text(&rendered);
        assert!(rendered.contains("There is no checkout."));
        assert!(rendered.contains("Report: text from 0 through 16 bytes, with these host-required fields:\n(none)"));
        assert!(!rendered.contains("Change:") && !rendered.contains("Verdict:"));
    }

    #[test]
    fn a_sub_agent_is_told_its_brief_its_tools_its_checkout_and_how_to_answer() {
        let mut charter = charter();
        charter.models = Box::new([crate::charter::Llm { model: bytes(b"model-b"), ..charter.llm.clone() }]);
        let families =
            Families { tools: Tools { inspect: true, modify: false, shell: false }, forge: false, agents: true };
        let expected: &[u8] = b"Find where tabs are parsed.

## Tools

You can read, list and search the files in the checkout.

## Checkout

- `temper`, which you may only read

## Sub-agents

You can ask for a sub-agent: an LLM of its own, working on a brief you write, with tools no wider than yours \
and a share of the budget. Its last message comes back to you as the result. It runs on the run's main LLM \
unless you name one of these: `model-b`.

## Answering

When you are done, end your turn with your answer: your last message goes, as it is, to the LLM that asked for \
you, and you are done.
";
        let found = Found::with_capacity(1);
        assert_eq!(text(&child(&charter, &found, b"Find where tabs are parsed.", families)), text(expected));
    }

    #[test]
    fn a_nudge_says_why_the_llm_stopped_and_how_many_nudges_are_left() {
        let rest = " Carry on with the work, and call `finish` when it is done. This is reminder 1 of 2: if you stop \
                    again after the last one, the run ends unfinished.";
        let cases = [
            (Stop::EndTurn, "You stopped without calling `finish`, so the work is not done."),
            (Stop::MaxTokens, "Your answer was cut off: it ran out of tokens."),
            (Stop::Refusal, "You declined to go on."),
            (Stop::NoCalls, "You said you would call a tool, and named none."),
        ];
        for (stop, first) in cases {
            let rendered = nudge(stop, 1, 2);
            let rendered = text(&rendered);
            assert!(rendered.starts_with(first) && rendered.ends_with(rest), "{rendered}");
            assert_eq!(rendered.len(), first.len() + rest.len());
        }
    }

    #[test]
    fn numbers_are_written_in_decimal() {
        for (n, expected) in [(0, "0"), (7, "7"), (10, "10"), (305, "305"), (u32::MAX, "4294967295")] {
            let mut measured = Text::measuring();
            measured.put_decimal(n);
            let mut written = measured.writing();
            written.put_decimal(n);
            assert_eq!(text(&written.finish()), expected);
        }
    }
}
