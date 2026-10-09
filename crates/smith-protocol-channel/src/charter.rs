//! Charter values become bounded wire policy (protocol/charter.md, sections 2
//! and 5). This translation retains no state, addresses or credentials;
//! `encode_charter` resolves domain endpoint names without admitting policy.
use crate::{Endpoints, Error};
use alloc::boxed::Box;
use skein_lib::{List, Writer};
use smith_charter as wire;
use smith_domain::run;

/// Encode every declared charter form using the configured endpoint names.
#[expect(
    clippy::manual_let_else,
    clippy::too_many_lines,
    reason = "total checked translation of the complete charter vocabulary"
)]
pub fn encode_charter(
    source: &run::Charter,
    limits: &wire::v2::Limits,
    endpoints: &Endpoints,
) -> Result<Box<[u8]>, Error> {
    let mut brief = List::with_capacity(count(source.brief.sections.len())?);
    for section in &source.brief.sections {
        if brief
            .push(wire::Section::new(
                limits,
                wire::SectionParts { title: section.title.clone(), body: section.text.clone() },
            )?)
            .is_err()
        {
            return Err(Error::ResultCapacity);
        }
    }
    let mut host = List::with_capacity(count(source.grants.host_tools.len())?);
    for tool in &source.grants.host_tools {
        let effect = match tool.effect {
            run::HostEffect::Read => wire::Effect::Read,
            run::HostEffect::Write => wire::Effect::Write,
        };
        let item = wire::HostTool::new(
            limits,
            wire::HostToolParts {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input: tool.schema.clone(),
                effect,
                deadline: tool.timeout,
            },
        )?;
        if host.push(item).is_err() {
            return Err(Error::ResultCapacity);
        }
    }
    let deliver = match &source.grants.deliver {
        Some(rule) => Some(change(rule, limits)?),
        None => None,
    };
    let tools = wire::Tools::new(
        limits,
        wire::ToolsParts {
            families: wire::Families::new(
                limits,
                wire::FamiliesParts {
                    skein_bools: [
                        source.grants.tools.inspect,
                        source.grants.tools.modify,
                        source.grants.tools.shell,
                        source.grants.agents,
                    ],
                },
            )?,
            wait: source.grants.wait,
            deliver,
            host,
        },
    )?;
    let mut verdicts = List::with_capacity(count(source.outcome.verdicts.len())?);
    for rule in &source.outcome.verdicts {
        let mut kinds = List::with_capacity(count(rule.items.kinds.len())?);
        for kind in &rule.items.kinds {
            let item = wire::ItemKind::new(
                limits,
                wire::ItemKindParts { kind: kind.kind.clone(), fields: fields(&kind.fields, limits)? },
            )?;
            if kinds.push(item).is_err() {
                return Err(Error::ResultCapacity);
            }
        }
        let item = wire::VerdictRule::new(
            limits,
            wire::VerdictRuleParts {
                label: rule.name.clone(),
                text_max: rule.text_max,
                fields: fields(&rule.fields, limits)?,
                least: rule.items.min,
                most: rule.items.max,
                kinds,
            },
        )?;
        if verdicts.push(item).is_err() {
            return Err(Error::ResultCapacity);
        }
    }
    let contract = wire::Contract::new(
        limits,
        wire::ContractParts {
            change: match &source.outcome.change {
                Some(rule) => Some(change(rule, limits)?),
                None => None,
            },
            report: match &source.outcome.report {
                Some(rule) => Some(text(rule, limits)?),
                None => None,
            },
            failure: match &source.outcome.failure {
                Some(rule) => Some(text(rule, limits)?),
                None => None,
            },
            verdicts,
        },
    )?;
    let conventions = match &source.conventions {
        Some(paths) => Some(wire::Conventions::new(
            limits,
            wire::ConventionsParts { guide: paths.guide.clone(), checks: paths.checks.clone() },
        )?),
        None => None,
    };
    let mut models = List::with_capacity(count(source.models.len())?);
    for model in &source.models {
        if models.push(llm(model, limits, endpoints)?).is_err() {
            return Err(Error::ResultCapacity);
        }
    }
    let record = wire::Charter::new(
        limits,
        wire::CharterParts {
            instructions: source.instructions.clone(),
            brief,
            tools,
            contract,
            conventions,
            budget: wire::Budget::new(
                limits,
                wire::BudgetParts { turns: source.budget.turns, spend: source.budget.spend, time: source.budget.time },
            )?,
            main: llm(&source.llm, limits, endpoints)?,
            models,
            waiting: source.waiting,
            resume: source.resume,
        },
    )?;
    let length = match usize::try_from(record.measure()) {
        Ok(length) => length,
        Err(_) => return Err(Error::ResultCapacity),
    };
    let mut writer = Writer::new(length);
    record.encode(&mut writer)?;
    Ok(writer.finish())
}

fn count(length: usize) -> Result<u32, Error> {
    match u32::try_from(length) {
        Ok(count) => Ok(count),
        Err(_) => Err(Error::ResultCapacity),
    }
}

fn fields(source: &[run::outcome::FieldRule], limits: &wire::v2::Limits) -> Result<List<wire::FieldRule>, Error> {
    let mut fields = List::with_capacity(count(source.len())?);
    for field in source {
        let item = wire::FieldRule::new(limits, wire::FieldRuleParts { name: field.name.clone(), max: field.max })?;
        if fields.push(item).is_err() {
            return Err(Error::ResultCapacity);
        }
    }
    Ok(fields)
}

fn change(source: &run::outcome::ChangeSpec, limits: &wire::v2::Limits) -> Result<wire::ChangeRule, Error> {
    Ok(wire::ChangeRule::new(
        limits,
        wire::ChangeRuleParts { checks_must_pass: source.checks_must_pass, fields: fields(&source.fields, limits)? },
    )?)
}

fn text(source: &run::outcome::TextSpec, limits: &wire::v2::Limits) -> Result<wire::TextRule, Error> {
    Ok(wire::TextRule::new(limits, wire::TextRuleParts { max: source.max, fields: fields(&source.fields, limits)? })?)
}

fn llm(source: &run::charter::Llm, limits: &wire::v2::Limits, endpoints: &Endpoints) -> Result<wire::Llm, Error> {
    let endpoint = match endpoints.name_of(source.endpoint.0, source.dialect) {
        Some(name) => Box::from(name),
        None => return Err(Error::Endpoints),
    };
    match endpoints.resolve(&endpoint) {
        Some(configured) if configured.account == source.account => {}
        Some(_) | None => return Err(Error::Endpoints),
    }
    Ok(wire::Llm::new(
        limits,
        wire::LlmParts {
            endpoint,
            model: source.model.clone(),
            window: source.window,
            output: source.output,
            prices: wire::Prices::new(
                limits,
                wire::PricesParts {
                    input: source.prices.input,
                    cached: source.prices.cached,
                    output: source.prices.output,
                    unit: source.prices.unit,
                },
            )?,
        },
    )?)
}
