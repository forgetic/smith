//! Schemas and descriptions for Smith's tools.
//! This module retains no state. The prompt translation offers these declarations
//! under the domain's grants; host declarations pass through unchanged.
//! Contract: protocol/llm.md, section 3; domain/tools.md, section 3.

use alloc::boxed::Box;

use skein_lib::{Decimal, Duration, List, Writer};
use skein_llm::Error;
use smith_domain::run;
use smith_domain::{llm, tools};

use crate::{ToolKind, ToolSchema};

/// The bounded inventory of Smith tools offered for this prompt.
/// Host tools are supplied separately by the charter and retain their bytes.
#[must_use]
pub fn schemas(prompt: &llm::Prompt) -> Box<[ToolSchema]> {
    let mut descriptors = List::with_capacity(10);
    if prompt.tools.inspect {
        add(&mut descriptors, descriptor(
            ToolKind::Owned(tools::Tool::Read),
            b"read",
            b"Read a workspace file. Read a file before changing it; paths stay within the workspace.",
            br#"{"type":"object","properties":{"path":{"type":"string"},"first_line":{"type":"integer","minimum":1},"lines":{"type":"integer","minimum":1}},"required":["path"]}"#,
        ));
        add(
            &mut descriptors,
            descriptor(
                ToolKind::Owned(tools::Tool::List),
                b"list",
                b"List entries of a directory within the workspace.",
                br#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}"#,
            ),
        );
        add(&mut descriptors, descriptor(
            ToolKind::Owned(tools::Tool::Search),
            b"search",
            b"Search workspace files for a pattern, optionally within one path or glob.",
            br#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"glob":{"type":"string"}},"required":["pattern"]}"#,
        ));
    }
    if prompt.tools.modify {
        add(&mut descriptors, descriptor(
            ToolKind::Owned(tools::Tool::Write),
            b"write",
            b"Write a workspace file. Read an existing file before changing it. A write runs alone.",
            br#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
        ));
        add(&mut descriptors, descriptor(
            ToolKind::Owned(tools::Tool::Edit),
            b"edit",
            b"Replace text in a workspace file already read. A write runs alone.",
            br#"{"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"},"all":{"type":"boolean"}},"required":["path","old","new"]}"#,
        ));
    }
    if prompt.tools.shell {
        add(&mut descriptors, descriptor(
            ToolKind::Owned(tools::Tool::Shell),
            b"shell",
            b"Run a command in the workspace, within its deadline. It may change files and runs alone.",
            br#"{"type":"object","properties":{"command":{"type":"string"},"timeout":{"type":"integer","minimum":1}},"required":["command"]}"#,
        ));
    }
    for offered in &prompt.served {
        match offered {
            llm::Served::Host(_) => {}
            llm::Served::Wait => add(&mut descriptors, descriptor(
                ToolKind::Wait,
                b"wait",
                b"Wait for the next message or outstanding result.",
                br#"{"type":"object","properties":{}}"#,
            )),
            llm::Served::SubAgent => add(&mut descriptors, descriptor(
                ToolKind::SubAgent,
                b"sub_agent",
                b"Ask a sub-agent to work under the granted tool families and optional model.",
                br#"{"type":"object","properties":{"brief":{"type":"string"},"tools":{"type":"array","items":{"type":"string","enum":["inspect","modify","shell"]}},"agents":{"type":"boolean"},"llm":{"type":"string"}},"required":["brief","tools"]}"#,
            )),
            llm::Served::Deliver => add(&mut descriptors, descriptor(
                ToolKind::Deliver,
                b"deliver",
                b"Offer the checked change to the host under the change contract.",
                br#"{"type":"object","properties":{"ticket":{"type":"string"}},"required":["ticket"]}"#,
            )),
            llm::Served::Finish => add(&mut descriptors, descriptor(
                ToolKind::Finish,
                b"finish",
                b"Declare the final result under the host's result contract.",
                br#"{"type":"object","properties":{"title":{"type":"string"},"body":{"type":"string"},"report":{"type":"string"},"source":{"type":"string"},"failure":{"type":"string"},"cause":{"type":"string"},"verdict":{"type":"string"},"children":{"type":"array","items":{"type":"object","properties":{"kind":{"type":"string"},"path":{"type":"string"}},"required":["kind","path"]}}},"anyOf":[{"required":["title","body"]},{"required":["report"]},{"required":["failure"]},{"required":["verdict","body"]}]}"#,
            )),
        }
    }
    descriptors.into_boxed()
}

/// The offered inventory with finish and deliver derived from the charter's
/// admitted rules. The caller supplies a separate deliver rule only when that
/// mid-run tool was granted.
pub fn schemas_for_contract(
    prompt: &llm::Prompt,
    outcome: &run::outcome::OutcomeSpec,
    deliver: Option<&run::outcome::ChangeSpec>,
    maximum: u32,
) -> Result<Box<[ToolSchema]>, Error> {
    let mut inventory = List::with_capacity(10);
    for mut descriptor in schemas(prompt) {
        match descriptor.kind {
            ToolKind::Finish => descriptor.schema = crate::contract::finish_schema(outcome, maximum)?,
            ToolKind::Deliver => {
                let change = deliver.ok_or(Error::Invalid)?;
                descriptor.schema = crate::contract::deliver_schema(change, maximum)?;
            }
            ToolKind::Owned(_) | ToolKind::SubAgent | ToolKind::Wait => {}
        }
        inventory.push(descriptor).or(Err(Error::Limit))?;
    }
    Ok(inventory.into_boxed())
}

fn add(descriptors: &mut List<ToolSchema>, schema: ToolSchema) {
    descriptors.push(schema).expect("ten fixed tool slots");
}

fn descriptor(kind: ToolKind, name: &[u8], description: &[u8], schema: &[u8]) -> ToolSchema {
    ToolSchema { kind, name: name.into(), description: description.into(), schema: schema.into() }
}

/// State the deployment's effective command deadlines in its offered inventory.
pub(crate) fn with_shell_deadlines(
    mut inventory: Box<[ToolSchema]>,
    default: Duration,
    maximum: Duration,
) -> Box<[ToolSchema]> {
    for descriptor in &mut inventory {
        match descriptor.kind {
            ToolKind::Owned(tool) => match tool {
                tools::Tool::Shell => descriptor.description = shell_description(default, maximum),
                tools::Tool::Read
                | tools::Tool::List
                | tools::Tool::Search
                | tools::Tool::Write
                | tools::Tool::Edit => {}
            },
            ToolKind::Finish | ToolKind::Deliver | ToolKind::SubAgent | ToolKind::Wait => {}
        }
    }
    inventory
}

fn shell_description(default: Duration, maximum: Duration) -> Box<[u8]> {
    let opening = b"Run a command in the workspace. It may change files and runs alone. timeout is seconds; the default deadline is ";
    let between = b" ms; the effective maximum is ";
    let ending = b" ms, also bounded by this session's remaining time.";
    let default = Decimal::of(default.as_nanos() / 1_000_000);
    let maximum = Decimal::of(maximum.as_nanos() / 1_000_000);
    let bytes = opening.len().checked_add(between.len()).expect("fixed words fit");
    let bytes = bytes.checked_add(ending.len()).expect("fixed words fit");
    let bytes = bytes.checked_add(default.as_bytes().len()).expect("one u64 decimal fits");
    let bytes = bytes.checked_add(maximum.as_bytes().len()).expect("two u64 decimals fit");
    let mut output = Writer::new(bytes);
    output.put(opening).expect("description room");
    output.put(default.as_bytes()).expect("description room");
    output.put(between).expect("description room");
    output.put(maximum.as_bytes()).expect("description room");
    output.put(ending).expect("description room");
    output.finish()
}
