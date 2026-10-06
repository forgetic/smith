//! Local choices translated into one agent charter (domain/host.md,
//! sections 5 and 8; domain/run.md, section 3.1). Configuration owns text,
//! model names and result policy, but neither credential secrets nor files.
//! `charter` copies those choices without consulting time or the workspace.

use alloc::boxed::Box;
use skein_lib::List;
use smith_domain::run::{self, charter, outcome};

use crate::Limits;

/// The person's chosen result form for this chat.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Contract {
    /// Finish with a report of at most the configured size.
    Report(outcome::TextSpec),
    /// Finish with a checked change in writable directories.
    Change(outcome::ChangeSpec),
}

/// One configured chat and the policy for each activation.
#[derive(Debug)]
pub struct Config {
    /// Store key for this chat.
    pub chat: Box<[u8]>,
    /// Host-authored role text.
    pub instructions: Box<[u8]>,
    /// Ordered host-authored context.
    pub brief: charter::Brief,
    /// Main model first, followed by models available to sub-agents.
    pub models: Box<[charter::Llm]>,
    /// Activation budget in the host's unit.
    pub budget: run::Budget,
    /// Optional guide and check paths.
    pub conventions: Option<run::Conventions>,
    /// Result form; a report is the usual choice.
    pub contract: Contract,
    /// Separately granted checked mid-run delivery, if configured.
    pub deliver: Option<outcome::ChangeSpec>,
    /// Required change field used as the first commit-message paragraph.
    pub title_field: Box<[u8]>,
    /// Duration a waiting run stays live before parking.
    pub waiting: skein_lib::Duration,
    /// Whether a saved transcript is supplied on start.
    pub resume: bool,
    /// Accounts for which grants must be fetched before a start.
    pub accounts: Box<[u32]>,
    /// Workspace authority; absent for a chat without files.
    pub workspace: Option<run::Workspace>,
}

/// Why configuration cannot be built within its receiving limits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Invalid {
    /// Chat name is empty or too large.
    Chat,
    /// Instructions or brief exceed their receiving bound.
    Text,
    /// There is no main model, or models exceed the count bound.
    Models,
    /// An account is missing, repeated or beyond capacity.
    Accounts,
    /// A model's endpoint is absent from the agent configuration.
    Endpoint,
    /// The agent cannot fit the chosen limits.
    Limits,
    /// A configured delivery has no bounded title field.
    Contract,
}

impl Config {
    /// Validate owned local policy before constructing the agent.
    pub fn validate(&self, limits: &Limits) -> Result<(), Invalid> {
        if self.chat.is_empty() || self.chat.len() > usize::try_from(limits.chat_bytes).expect("u32 fits usize") {
            return Err(Invalid::Chat);
        }
        if self.instructions.len() > usize::try_from(limits.text_bytes).expect("u32 fits usize") {
            return Err(Invalid::Text);
        }
        if self.title_field.is_empty()
            || self.title_field.len() > usize::try_from(limits.text_bytes).expect("u32 fits usize")
        {
            return Err(Invalid::Text);
        }
        if limits.line_bytes > limits.agent.run.message_bytes {
            return Err(Invalid::Limits);
        }
        match &self.contract {
            Contract::Change(spec) => {
                if !has_title(spec, &self.title_field) {
                    return Err(Invalid::Contract);
                }
            }
            Contract::Report(_) => {}
        }
        if let Some(spec) = &self.deliver
            && !has_title(spec, &self.title_field)
        {
            return Err(Invalid::Contract);
        }
        let mut brief_bytes = 0_u64;
        for section in &self.brief.sections {
            let Ok(title) = u64::try_from(section.title.len()) else {
                return Err(Invalid::Text);
            };
            let Ok(text) = u64::try_from(section.text.len()) else {
                return Err(Invalid::Text);
            };
            brief_bytes = brief_bytes.checked_add(title).ok_or(Invalid::Text)?;
            brief_bytes = brief_bytes.checked_add(text).ok_or(Invalid::Text)?;
        }
        if brief_bytes > u64::from(limits.text_bytes) {
            return Err(Invalid::Text);
        }
        if self.models.is_empty()
            || self.models.len() > usize::try_from(limits.models).expect("u32 fits usize")
            || self.models.len() > usize::try_from(limits.agent.run.models.saturating_add(1)).expect("u32 fits usize")
        {
            return Err(Invalid::Models);
        }
        if limits.endpoints.len() > usize::try_from(limits.agent.endpoints).expect("u32 fits usize") {
            return Err(Invalid::Endpoint);
        }
        for (index, endpoint) in limits.endpoints.iter().enumerate() {
            if limits.endpoints.get(index.saturating_add(1)..).unwrap_or_default().contains(endpoint) {
                return Err(Invalid::Endpoint);
            }
        }
        if self.accounts.len() > usize::try_from(limits.agent.accounts).expect("u32 fits usize") {
            return Err(Invalid::Accounts);
        }
        if let Some(workspace) = &self.workspace
            && (workspace.directories.is_empty()
                || workspace.directories.len() > usize::try_from(limits.agent.run.directories).expect("u32 fits usize"))
        {
            return Err(Invalid::Limits);
        }
        for (index, account) in self.accounts.iter().enumerate() {
            if self.accounts.get(index.saturating_add(1)..).unwrap_or_default().contains(account) {
                return Err(Invalid::Accounts);
            }
        }
        for model in &self.models {
            if !self.accounts.contains(&model.account) {
                return Err(Invalid::Accounts);
            }
            if !limits.endpoints.contains(&model.endpoint) {
                return Err(Invalid::Endpoint);
            }
        }
        if crate::worst_case(limits).is_none() {
            return Err(Invalid::Limits);
        }
        Ok(())
    }
}

fn has_title(spec: &outcome::ChangeSpec, title: &[u8]) -> bool {
    for field in &spec.fields {
        if field.name.as_ref() == title && field.max > 0 {
            return true;
        }
    }
    false
}

/// Translate local choices into the agent's charter for one start.
#[must_use]
pub fn charter(config: &Config) -> charter::Charter {
    let workspace = config.workspace.as_ref();
    let mut writable = false;
    if let Some(workspace) = workspace {
        for directory in &workspace.directories {
            if directory.writable {
                writable = true;
            }
        }
    }
    let report = match &config.contract {
        Contract::Report(spec) => Some(spec.clone()),
        Contract::Change(_) => None,
    };
    let change = match &config.contract {
        Contract::Report(_) => None,
        Contract::Change(spec) => Some(spec.clone()),
    };
    let main = config.models.first().expect("validated configuration has a main model").clone();
    let models = Box::from(config.models.get(1..).unwrap_or_default());
    let mut sections = List::with_capacity(u32::try_from(config.brief.sections.len()).expect("validated brief count"));
    for section in &config.brief.sections {
        sections
            .push(charter::Section { title: section.title.clone(), text: section.text.clone() })
            .expect("validated brief capacity");
    }
    charter::Charter {
        resume: config.resume,
        waiting: config.waiting,
        instructions: config.instructions.clone(),
        brief: charter::Brief { sections: sections.into_boxed() },
        conventions: config.conventions.clone(),
        grants: charter::Grants {
            wait: true,
            deliver: config.deliver.clone(),
            tools: charter::Tools { inspect: workspace.is_some(), modify: writable, shell: workspace.is_some() },
            agents: false,
            host_tools: Box::new([]),
        },
        outcome: outcome::OutcomeSpec { change, verdicts: Box::new([]), report, failure: None },
        budget: config.budget,
        llm: main,
        models,
    }
}
