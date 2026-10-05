//! The application world's actual prepared shared Client and scripted byte peer.
//! Application descriptors/decoding stay here; transport and provider grammar
//! belong entirely to Skein. Existing World owns clocks, root and checkout work.

use skein_lib::Token;
use skein_llm::{self as shared, client};
use skein_llm_world::fake::Exchange;
use smith_domain::{Event, llm, tools};
use smith_protocol_llm::{self as adapter, Context, Limits, Receiving, ResolvedCall, ToolKind, ToolSchema};

/// Caller-supplied endpoint and credential data for the real wire fixture.
#[expect(missing_debug_implementations, reason = "fixture owns caller credential bytes")]
pub struct Configuration {
    /// Whole shared endpoint configuration, supplied as neutral caller data.
    pub endpoint: shared::Endpoint,
    /// Actual grant supplied to one Client and separately to its independent peer.
    pub credential: shared::Credential,
    /// Independent outside expectation for a native error result's text prefix.
    pub error_prefix: Box<[u8]>,
    /// Independent outside expectation for a native error result's parsed flag.
    pub error_flag: bool,
}

/// Native configurations are shared kit data, with no Smith dialect branching.
#[must_use]
pub fn configurations() -> Box<[Configuration]> {
    Box::new([
        Configuration {
            endpoint: shared::Endpoint::codex(),
            credential: shared::Credential {
                access_token: b"fixture-token".as_slice().into(),
                account_id: b"fixture-account".as_slice().into(),
            },
            error_prefix: b"Error: ".as_slice().into(),
            error_flag: false,
        },
        Configuration {
            endpoint: shared::Endpoint::anthropic(),
            credential: shared::Credential::anthropic(b"fixture-token".as_slice().into()),
            error_prefix: Box::new([]),
            error_flag: true,
        },
    ])
}

/// Finite actual lower chronology, without retaining another completion body.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Observed {
    Completed(Token),
    Failed(Token),
    Cancelled(Token),
    Reusable,
    Close,
    Closed,
}

/// Owns the actual prepared machine and retained adapter context through settlement.
pub struct Wire {
    /// The one actual Client, lower HTTP/SSE peer and shared script domain.
    pub peer: Exchange,
    /// At most one terminal plus Reusable/Close/Closed, without diagnostic bodies.
    pub observed: Vec<Observed>,
    context: Option<Context>,
}

impl core::fmt::Debug for Wire {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Wire")
            .field("waiting", &self.peer.machine.waiting())
            .field("observed", &self.observed)
            .field("live_context", &self.context.is_some())
            .finish()
    }
}

impl Wire {
    /// The root's actual Complete request supplies owner/prompt/receiving metadata.
    /// Preparation is effect free; start is a separate real Client entrance.
    pub fn prepare(
        owner: Token,
        prompt: llm::Prompt,
        receiving: Receiving,
        configuration: &Configuration,
        limits: &Limits,
        scripts: Box<[skein_fake_llm_domain::api::Script]>,
    ) -> Result<Self, adapter::Error> {
        let endpoint_name = prompt.endpoint;
        let application = schemas(&prompt);
        let endpoint = configuration.endpoint.clone();
        let peer_endpoint = endpoint.clone();
        let credential = shared::Credential {
            access_token: configuration.credential.access_token.clone(),
            account_id: configuration.credential.account_id.clone(),
        };
        let peer_credential = shared::Credential {
            access_token: credential.access_token.clone(),
            account_id: credential.account_id.clone(),
        };
        let adapter::Prepared { client, context } = adapter::prepare(
            adapter::Input {
                owner,
                prompt,
                endpoint_name,
                endpoint,
                credential,
                application,
                results: Box::new([]),
                receiving,
            },
            limits,
        )?;
        let peer = Exchange::prepared(client, peer_endpoint, peer_credential, limits.client, scripts);
        Ok(Self { peer, context: Some(context), observed: Vec::new() })
    }

    pub fn start(&mut self) -> Vec<Event> {
        self.peer.start();
        self.take()
    }

    /// One real shared-world progress step. The root world may interleave timers,
    /// actual cancellation requests and other delegated effects between steps.
    pub fn tick(&mut self) -> (bool, Vec<Event>) {
        let progress = self.peer.tick(true);
        (progress, self.take())
    }

    pub fn cancel(&mut self) -> Vec<Event> {
        self.peer.request(client::Request::Cancel);
        self.take()
    }

    pub fn close(&mut self) -> Vec<Event> {
        self.peer.request(client::Request::Close);
        self.take()
    }

    /// Called only after the world's observed lower Close has actually settled.
    pub fn settle(&mut self) -> Vec<Event> {
        self.peer.settle();
        self.take()
    }

    fn take(&mut self) -> Vec<Event> {
        let mut returned = Vec::new();
        for event in core::mem::take(&mut self.peer.seen) {
            match event {
                client::Event::Completed { owner, completion } => {
                    self.observed.push(Observed::Completed(owner));
                    let context = self.context.take().expect("one actual Client terminal owns the context");
                    let decoded = resolutions(&context, &completion);
                    returned.push(
                        adapter::completion(context, owner, completion, decoded)
                            .expect("actual admitted Client completion and explicit application codec fit receiving"),
                    );
                }
                client::Event::Failed { owner, failure, evidence, detail } => {
                    self.observed.push(Observed::Failed(owner));
                    let context = self.context.take().expect("one actual Client terminal owns the context");
                    returned.push(
                        adapter::failed(context, owner, failure, evidence, detail).expect(
                            "actual admitted failure retains owner, class, evidence and full bounded diagnostic",
                        ),
                    );
                }
                client::Event::Cancelled { owner } => {
                    self.observed.push(Observed::Cancelled(owner));
                    let context = self.context.take().expect("one actual Client terminal owns the context");
                    returned.push(adapter::cancelled(context, owner).expect("actual cancellation callback owner"));
                }
                client::Event::Reusable => self.observed.push(Observed::Reusable),
                client::Event::Close => self.observed.push(Observed::Close),
                client::Event::Closed => self.observed.push(Observed::Closed),
                client::Event::Delta { .. } | client::Event::Block { .. } => {}
            }
        }
        assert!(self.observed.len() <= 4, "one actual terminal and finite lower settlement chronology");
        assert!(returned.len() <= 1, "one root terminal per actual wire call");
        returned
    }
}

fn resolutions(context: &Context, completion: &shared::Completion) -> Box<[ResolvedCall]> {
    completion
        .content
        .iter()
        .enumerate()
        .filter_map(|(position, block)| match block {
            shared::Block::ToolCall { name, arguments, .. } => {
                if context.served().iter().any(|declaration| match declaration {
                    llm::Served::Host(tool) => tool.name == *name,
                    llm::Served::Finish | llm::Served::Deliver | llm::Served::SubAgent | llm::Served::Wait => false,
                }) {
                    return None;
                }
                Some(ResolvedCall {
                    position: u32::try_from(position).expect("bounded actual assistant position"),
                    name: name.clone(),
                    input: arguments.clone(),
                    call: crate::translate::decode(name, arguments, context.grants(), context.served()),
                })
            }
            shared::Block::Text { .. }
            | shared::Block::Refusal { .. }
            | shared::Block::Reasoning { .. }
            | shared::Block::ToolResult { .. } => None,
        })
        .collect()
}

/// Explicit whole schemas for this world's finite application fixture language.
/// These are caller data, not provider grammar or production schema policy.
#[must_use]
pub fn schemas(prompt: &llm::Prompt) -> Box<[ToolSchema]> {
    let mut descriptors = Vec::new();
    if prompt.tools.inspect {
        descriptors.extend([
            descriptor(ToolKind::Owned(tools::Tool::Read), b"read", br#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}"#),
            descriptor(ToolKind::Owned(tools::Tool::List), b"list", br#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}"#),
            descriptor(ToolKind::Owned(tools::Tool::Search), b"search", br#"{"type":"object","properties":{"path":{"type":"string"},"pattern":{"type":"string"}},"required":["path","pattern"],"additionalProperties":false}"#),
        ]);
    }
    if prompt.tools.modify {
        descriptors.extend([
            descriptor(ToolKind::Owned(tools::Tool::Write), b"write", br#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false}"#),
            descriptor(ToolKind::Owned(tools::Tool::Edit), b"edit", br#"{"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"}},"required":["path","old","new"],"additionalProperties":false}"#),
        ]);
    }
    if prompt.tools.shell {
        descriptors.push(descriptor(ToolKind::Owned(tools::Tool::Shell), b"shell", br#"{"type":"object","properties":{"command":{"type":"string"}},"required":["command"],"additionalProperties":false}"#));
    }
    for served in &prompt.served {
        match served {
            llm::Served::Host(_) => {}
            llm::Served::Wait => descriptors.push(descriptor(ToolKind::Wait, b"wait", br#"{"type":"object","properties":{},"additionalProperties":false}"#)),
            llm::Served::Deliver => descriptors.push(descriptor(ToolKind::Deliver, b"deliver", br#"{"type":"object","properties":{"ticket":{"type":"string"}},"required":["ticket"],"additionalProperties":false}"#)),
            llm::Served::Finish => descriptors.push(descriptor(ToolKind::Finish, b"finish", br#"{"type":"object","properties":{"title":{"type":"string"},"body":{"type":"string"},"report":{"type":"string"},"source":{"type":"string"},"failure":{"type":"string"},"cause":{"type":"string"},"verdict":{"type":"string"},"children":{"type":"array","items":{"type":"object","properties":{"kind":{"type":"string"},"path":{"type":"string"}},"required":["kind","path"],"additionalProperties":false}}},"anyOf":[{"required":["title","body"]},{"required":["report"]},{"required":["failure"]},{"required":["verdict","body"]}],"additionalProperties":false}"#)),
            llm::Served::SubAgent => descriptors.push(descriptor(ToolKind::SubAgent, b"sub_agent", br#"{"type":"object","properties":{"brief":{"type":"string"},"tools":{"type":"array","items":{"type":"string","enum":["inspect","modify","shell"]}},"agents":{"type":"boolean"},"llm":{"type":"string"}},"required":["brief","tools"],"additionalProperties":false}"#)),
        }
    }
    descriptors.into()
}

fn descriptor(kind: ToolKind, name: &[u8], schema: &[u8]) -> ToolSchema {
    ToolSchema {
        kind,
        name: name.into(),
        description: b"Finite application world tool.".as_slice().into(),
        schema: schema.into(),
    }
}
