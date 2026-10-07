//! Charter v1 translation into the run's policy data (protocol/charter.md,
//! sections 2, 5 and 7; protocol/agent.md, section 4).
//!
//! The caller supplies configured endpoint names and their domain identities.
//! This module keeps no state, knows no addresses or credential values, and
//! returns either owned domain policy or a typed invalid-start reason.

use alloc::boxed::Box;
use skein_lib::{List, Reader};
use smith_charter as wire;
use smith_domain::run;

/// One configured endpoint name and its domain-visible identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// Byte name carried by charters and transcripts.
    pub name: Box<[u8]>,
    /// Domain-visible endpoint identity.
    pub number: u32,
    /// Opaque replay dialect identity.
    pub dialect: u32,
    /// Credential account name; the value stays below the domain.
    pub account: u32,
}

/// Bounded endpoint names resolved before an agent starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    entries: List<Endpoint>,
}

impl Endpoints {
    /// Retain the checked startup endpoint table.
    #[must_use]
    pub fn new(entries: List<Endpoint>) -> Endpoints {
        Endpoints { entries }
    }

    /// Number of names retained by the agent.
    #[must_use]
    pub(crate) fn len(&self) -> u32 {
        self.entries.len()
    }

    /// Whether the configured names fit the channel's endpoint allowance.
    #[must_use]
    pub(crate) fn fits(&self, count: u32) -> bool {
        if self.len() > count {
            return false;
        }
        for endpoint in &self.entries {
            match u32::try_from(endpoint.name.len()) {
                Ok(length) if length <= wire::CEILINGS.llm_endpoint => {}
                Ok(_) | Err(_) => return false,
            }
        }
        true
    }

    /// Find a charter name without exposing endpoint addresses.
    #[must_use]
    #[expect(clippy::manual_find, reason = "production steps do not use closures")]
    pub fn resolve(&self, name: &[u8]) -> Option<&Endpoint> {
        for endpoint in &self.entries {
            if endpoint.name.as_ref() == name {
                return Some(endpoint);
            }
        }
        None
    }
}

/// Decode one charter and resolve all LLM endpoint names before admission.
#[expect(clippy::manual_let_else, reason = "explicit exhaustive result handling")]
pub fn decode_charter(
    bytes: &[u8],
    limits: &wire::v1::Limits,
    endpoints: &Endpoints,
) -> Result<run::Charter, run::Invalid> {
    if bytes.len() < 2 {
        return Err(run::Invalid::MalformedCharter);
    }
    if bytes.get(..2) != Some(&[0, 1][..]) {
        return Err(run::Invalid::CharterVersion);
    }
    let charter = match wire::Charter::decode(limits, &mut Reader::new(bytes)) {
        Ok(charter) => charter,
        Err(_) => return Err(run::Invalid::MalformedCharter),
    };
    translate_charter(&charter, endpoints)
}

#[expect(clippy::manual_map, reason = "production steps do not use closures")]
fn translate_charter(charter: &wire::Charter, endpoints: &Endpoints) -> Result<run::Charter, run::Invalid> {
    let mut sections = List::with_capacity(charter.brief().len());
    for section in charter.brief() {
        let item = run::Section { title: Box::from(section.title()), text: Box::from(section.body()) };
        if sections.push(item).is_err() {
            return Err(run::Invalid::TooLarge);
        }
    }

    let tools = charter.tools();
    let families = tools.families();
    let mut host_tools = List::with_capacity(tools.host().len());
    for tool in tools.host() {
        let effect = match tool.effect() {
            wire::Effect::Read => run::HostEffect::Read,
            wire::Effect::Write => run::HostEffect::Write,
        };
        let item = run::HostTool {
            name: Box::from(tool.name()),
            description: Box::from(tool.description()),
            schema: Box::from(tool.input()),
            effect,
            timeout: tool.deadline(),
        };
        if host_tools.push(item).is_err() {
            return Err(run::Invalid::TooLarge);
        }
    }

    let deliver = match tools.deliver() {
        Some(rule) => Some(change_rule(rule)?),
        None => None,
    };
    let conventions = match charter.conventions() {
        Some(paths) => Some(run::Conventions { guide: Box::from(paths.guide()), checks: Box::from(paths.checks()) }),
        None => None,
    };
    let mut models = List::with_capacity(charter.models().len());
    for model in charter.models() {
        if models.push(llm(model, endpoints)?).is_err() {
            return Err(run::Invalid::TooLarge);
        }
    }
    let budget = charter.budget();
    Ok(run::Charter {
        resume: charter.resume(),
        waiting: charter.waiting(),
        instructions: Box::from(charter.instructions()),
        brief: run::Brief { sections: sections.into_boxed() },
        conventions,
        grants: run::charter::Grants {
            wait: tools.wait(),
            deliver,
            tools: run::charter::Tools {
                inspect: families.inspect(),
                modify: families.modify(),
                shell: families.shell(),
            },
            agents: families.agents(),
            host_tools: host_tools.into_boxed(),
        },
        outcome: contract(charter.contract())?,
        budget: run::Budget { turns: budget.turns(), spend: budget.spend(), time: budget.time() },
        llm: llm(charter.main(), endpoints)?,
        models: models.into_boxed(),
    })
}

#[expect(clippy::manual_let_else, reason = "explicit exhaustive option handling")]
fn llm(value: &wire::Llm, endpoints: &Endpoints) -> Result<run::charter::Llm, run::Invalid> {
    let endpoint = match endpoints.resolve(value.endpoint()) {
        Some(endpoint) => endpoint,
        None => return Err(run::Invalid::Endpoint),
    };
    let prices = value.prices();
    Ok(run::charter::Llm {
        prices: run::Prices {
            input: prices.input(),
            cached: prices.cached(),
            output: prices.output(),
            unit: prices.unit(),
        },
        dialect: endpoint.dialect,
        account: endpoint.account,
        endpoint: run::charter::Endpoint(endpoint.number),
        model: Box::from(value.model()),
        max_tokens: value.max_tokens(),
    })
}

fn contract(value: &wire::Contract) -> Result<run::outcome::OutcomeSpec, run::Invalid> {
    let report = match value.report() {
        Some(rule) => Some(text_rule(rule)?),
        None => None,
    };
    let change = match value.change() {
        Some(rule) => Some(change_rule(rule)?),
        None => None,
    };
    let failure = match value.failure() {
        Some(rule) => Some(text_rule(rule)?),
        None => None,
    };
    let mut verdicts = List::with_capacity(value.verdicts().len());
    for rule in value.verdicts() {
        let mut kinds = List::with_capacity(rule.kinds().len());
        for kind in rule.kinds() {
            let item = run::outcome::ItemRule { kind: Box::from(kind.kind()), fields: field_rules(kind.fields())? };
            if kinds.push(item).is_err() {
                return Err(run::Invalid::TooLarge);
            }
        }
        let item = run::outcome::VerdictRule {
            name: Box::from(rule.label()),
            text_max: rule.text_max(),
            fields: field_rules(rule.fields())?,
            items: run::outcome::ItemSpec { min: rule.least(), max: rule.most(), kinds: kinds.into_boxed() },
        };
        if verdicts.push(item).is_err() {
            return Err(run::Invalid::TooLarge);
        }
    }
    Ok(run::outcome::OutcomeSpec { change, verdicts: verdicts.into_boxed(), report, failure })
}

fn text_rule(value: &wire::TextRule) -> Result<run::outcome::TextSpec, run::Invalid> {
    Ok(run::outcome::TextSpec { max: value.max(), fields: field_rules(value.fields())? })
}

fn change_rule(value: &wire::ChangeRule) -> Result<run::outcome::ChangeSpec, run::Invalid> {
    Ok(run::outcome::ChangeSpec { checks_must_pass: value.checks_must_pass(), fields: field_rules(value.fields())? })
}

fn field_rules(value: &List<wire::FieldRule>) -> Result<Box<[run::outcome::FieldRule]>, run::Invalid> {
    let mut fields = List::with_capacity(value.len());
    for field in value {
        let item = run::outcome::FieldRule { name: Box::from(field.name()), max: field.max() };
        if fields.push(item).is_err() {
            return Err(run::Invalid::TooLarge);
        }
    }
    Ok(fields.into_boxed())
}
