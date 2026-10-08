//! Local-host settings, read before a chat opens (protocol/hosts.md, section
//! 5.2). The shell keeps the operator's JSON and an optional workspace
//! override. Neither file is a transcript or a credential store. The merged
//! agent object is written as the child agent's configuration at startup.

use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;
use skein_lib::{Duration, List, Token, Writer};
use smith_charter as wire;
use smith_domain::{self as domain, run};
use smith_local_domain as local;
use smith_protocol_channel as channel;

const SETTINGS_BYTES: u64 = 1 << 20;

/// One priced model from a named endpoint in the generated agent config.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub endpoint: String,
    pub name: String,
    pub max_tokens: u32,
    pub input_price: u64,
    pub cached_price: u64,
    pub output_price: u64,
    pub price_unit: u32,
}

/// Host-selected command and result bounds for one activation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub turns: u32,
    pub spend: u64,
    pub seconds: u64,
}

/// One configured directory mounted into a local run.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Directory {
    pub name: String,
    pub path: String,
    pub writable: bool,
    pub git: bool,
}

/// A required field in a change result or mid-run delivery.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub name: String,
    pub max: u32,
}

/// Configured final result form; report is the default when omitted.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "form", rename_all = "snake_case", deny_unknown_fields)]
pub enum Contract {
    Report { max: u32 },
    Change { checks_must_pass: bool, fields: Vec<Field> },
}

/// Paths used by guide and check operations in the agent workspace.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conventions {
    pub guide: String,
    pub checks: String,
}

/// Host-authored context sent in order with every activation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub title: String,
    pub text: String,
}

/// Local settings after the optional workspace override is merged.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub agent: Value,
    pub chat: String,
    pub instructions: String,
    pub models: Vec<Model>,
    pub budget: Budget,
    pub waiting_seconds: u64,
    #[serde(default)]
    pub brief: Vec<Section>,
    #[serde(default)]
    pub directories: Vec<Directory>,
    #[serde(default)]
    pub conventions: Option<Conventions>,
    #[serde(default)]
    pub contract: Option<Contract>,
    #[serde(default)]
    pub deliver: Option<Vec<Field>>,
    #[serde(default = "default_title")]
    pub title_field: String,
}

/// Build bounded local policy and parallel workspace paths from settings.
pub struct Prepared {
    pub config: local::Config,
    pub limits: local::Limits,
    pub paths: Box<[Box<[u8]>]>,
}

pub fn policy(
    settings: &Settings,
    endpoints: &channel::Endpoints,
    agent_limits: domain::Limits,
) -> Result<Prepared, String> {
    let mut models = Vec::with_capacity(settings.models.len());
    let mut accounts = Vec::new();
    for model in &settings.models {
        let endpoint = endpoints.resolve(model.endpoint.as_bytes()).ok_or("local model endpoint is absent")?;
        if !accounts.contains(&endpoint.account) {
            accounts.push(endpoint.account);
        }
        models.push(run::charter::Llm {
            prices: run::Prices {
                input: model.input_price,
                cached: model.cached_price,
                output: model.output_price,
                unit: model.price_unit,
            },
            dialect: endpoint.dialect,
            account: endpoint.account,
            endpoint: run::charter::Endpoint(endpoint.number),
            model: model.name.as_bytes().into(),
            max_tokens: model.max_tokens,
        });
    }
    let mut paths = Vec::with_capacity(settings.directories.len());
    let mut directories = Vec::with_capacity(settings.directories.len());
    for (index, directory) in settings.directories.iter().enumerate() {
        let number = u64::try_from(index)
            .map_err(|_| "too many workspace directories")?
            .checked_add(1)
            .ok_or("directory token overflow")?;
        paths.push(directory.path.as_bytes().into());
        directories.push(run::Directory {
            name: directory.name.as_bytes().into(),
            root: Token::new(number),
            writable: directory.writable,
            git: directory.git,
            conflicts: Box::new([]),
        });
    }
    let workspace =
        if directories.is_empty() { None } else { Some(run::Workspace { directories: directories.into() }) };
    let mut brief = Vec::with_capacity(settings.brief.len());
    for section in &settings.brief {
        brief.push(run::charter::Section {
            title: section.title.as_bytes().into(),
            text: section.text.as_bytes().into(),
        });
    }
    let contract = match &settings.contract {
        Some(Contract::Change { checks_must_pass, fields }) => local::Contract::Change(run::outcome::ChangeSpec {
            checks_must_pass: *checks_must_pass,
            fields: fields_of(fields),
        }),
        Some(Contract::Report { max }) => {
            local::Contract::Report(run::outcome::TextSpec { max: *max, fields: Box::new([]) })
        }
        None => local::Contract::Report(run::outcome::TextSpec { max: 4096, fields: Box::new([]) }),
    };
    let deliver = settings
        .deliver
        .as_ref()
        .map(|fields| run::outcome::ChangeSpec { checks_must_pass: true, fields: fields_of(fields) });
    let policy = local::Config {
        chat: settings.chat.as_bytes().into(),
        instructions: settings.instructions.as_bytes().into(),
        brief: run::charter::Brief { sections: brief.into() },
        models: models.into(),
        budget: run::Budget {
            turns: settings.budget.turns,
            spend: settings.budget.spend,
            time: Duration::from_secs(settings.budget.seconds),
        },
        conventions: settings.conventions.as_ref().map(|value| run::Conventions {
            guide: value.guide.as_bytes().into(),
            checks: value.checks.as_bytes().into(),
        }),
        contract,
        deliver,
        title_field: settings.title_field.as_bytes().into(),
        waiting: Duration::from_secs(settings.waiting_seconds),
        resume: true,
        accounts: accounts.into(),
        workspace,
        push: None,
    };
    let mut configured_endpoints = Vec::with_capacity(settings.models.len());
    for model in &settings.models {
        let endpoint = endpoints.resolve(model.endpoint.as_bytes()).expect("checked endpoint");
        let name = run::charter::Endpoint(endpoint.number);
        if !configured_endpoints.contains(&name) {
            configured_endpoints.push(name);
        }
    }
    let limits = local::Limits {
        endpoints: configured_endpoints.into(),
        agent: agent_limits,
        chat_bytes: 256,
        text_bytes: 1 << 16,
        models: 3,
        line_bytes: 4096,
        show_bytes: 8192,
        lines: 8,
        unsaved: 2,
        facts: 1024,
    };
    policy.validate(&limits).map_err(|error| format!("local policy: {error:?}"))?;
    Ok(Prepared { config: policy, limits, paths: paths.into() })
}

fn fields_of(source: &[Field]) -> Box<[run::outcome::FieldRule]> {
    source.iter().map(|field| run::outcome::FieldRule { name: field.name.as_bytes().into(), max: field.max }).collect()
}

/// Encode the local domain's chosen charter in the version-one wire family.
/// Decoding it with the configured endpoint table must recover this policy.
pub fn charter(policy: &local::Config, endpoints: &channel::Endpoints) -> Result<Box<[u8]>, String> {
    let source = local::charter(policy);
    let bounds = &wire::CEILINGS;
    let mut brief =
        List::with_capacity(u32::try_from(source.brief.sections.len()).map_err(|_| "too many brief sections")?);
    for section in &source.brief.sections {
        brief
            .push(
                wire::Section::new(
                    bounds,
                    wire::SectionParts { title: section.title.clone(), body: section.text.clone() },
                )
                .map_err(|error| format!("charter section: {error:?}"))?,
            )
            .map_err(|_| "too many brief sections")?;
    }
    let tools = wire::Tools::new(
        bounds,
        wire::ToolsParts {
            families: wire::Families::new(
                bounds,
                wire::FamiliesParts {
                    skein_bools: [
                        source.grants.tools.inspect,
                        source.grants.tools.modify,
                        source.grants.tools.shell,
                        source.grants.agents,
                    ],
                },
            )
            .map_err(|error| format!("charter families: {error:?}"))?,
            wait: source.grants.wait,
            deliver: source.grants.deliver.as_ref().map(change_rule).transpose()?,
            host: List::with_capacity(0),
        },
    )
    .map_err(|error| format!("charter tools: {error:?}"))?;
    let contract = wire::Contract::new(
        bounds,
        wire::ContractParts {
            report: source.outcome.report.as_ref().map(text_rule).transpose()?,
            verdicts: List::with_capacity(0),
            change: source.outcome.change.as_ref().map(change_rule).transpose()?,
            failure: None,
        },
    )
    .map_err(|error| format!("charter contract: {error:?}"))?;
    let conventions = source
        .conventions
        .as_ref()
        .map(|paths| {
            wire::Conventions::new(
                bounds,
                wire::ConventionsParts { guide: paths.guide.clone(), checks: paths.checks.clone() },
            )
            .map_err(|error| format!("charter conventions: {error:?}"))
        })
        .transpose()?;
    let budget = wire::Budget::new(
        bounds,
        wire::BudgetParts { turns: source.budget.turns, spend: source.budget.spend, time: source.budget.time },
    )
    .map_err(|error| format!("charter budget: {error:?}"))?;
    let main = llm(&source.llm, endpoints)?;
    let mut models = List::with_capacity(u32::try_from(source.models.len()).map_err(|_| "too many charter models")?);
    for model in &source.models {
        models.push(llm(model, endpoints)?).map_err(|_| "too many charter models")?;
    }
    let charter = wire::Charter::new(
        bounds,
        wire::CharterParts {
            instructions: source.instructions,
            brief,
            tools,
            contract,
            conventions,
            budget,
            main,
            models,
            waiting: source.waiting,
            resume: source.resume,
        },
    )
    .map_err(|error| format!("charter: {error:?}"))?;
    let mut writer = Writer::new(usize::try_from(charter.measure()).map_err(|_| "charter byte count overflow")?);
    charter.encode(&mut writer).map_err(|_| "charter encoder overflow")?;
    let bytes = writer.finish();
    let decoded =
        channel::decode_charter(&bytes, bounds, endpoints).map_err(|error| format!("charter decode: {error:?}"))?;
    if decoded != local::charter(policy) {
        return Err("encoded charter differs from local policy".into());
    }
    Ok(bytes)
}

fn llm(source: &run::charter::Llm, endpoints: &channel::Endpoints) -> Result<wire::Llm, String> {
    let bounds = &wire::CEILINGS;
    let name = endpoints.name_of(source.endpoint.0, source.dialect).ok_or("model endpoint name is absent")?;
    wire::Llm::new(
        bounds,
        wire::LlmParts {
            endpoint: name.into(),
            model: source.model.clone(),
            max_tokens: source.max_tokens,
            prices: wire::Prices::new(
                bounds,
                wire::PricesParts {
                    input: source.prices.input,
                    cached: source.prices.cached,
                    output: source.prices.output,
                    unit: source.prices.unit,
                },
            )
            .map_err(|error| format!("model prices: {error:?}"))?,
        },
    )
    .map_err(|error| format!("model: {error:?}"))
}

fn field_rules(source: &[run::outcome::FieldRule]) -> Result<List<wire::FieldRule>, String> {
    let mut fields = List::with_capacity(u32::try_from(source.len()).map_err(|_| "too many contract fields")?);
    for field in source {
        fields
            .push(
                wire::FieldRule::new(
                    &wire::CEILINGS,
                    wire::FieldRuleParts { name: field.name.clone(), max: field.max },
                )
                .map_err(|error| format!("contract field: {error:?}"))?,
            )
            .map_err(|_| "too many contract fields")?;
    }
    Ok(fields)
}

fn change_rule(source: &run::outcome::ChangeSpec) -> Result<wire::ChangeRule, String> {
    wire::ChangeRule::new(
        &wire::CEILINGS,
        wire::ChangeRuleParts { checks_must_pass: source.checks_must_pass, fields: field_rules(&source.fields)? },
    )
    .map_err(|error| format!("change rule: {error:?}"))
}

fn text_rule(source: &run::outcome::TextSpec) -> Result<wire::TextRule, String> {
    wire::TextRule::new(&wire::CEILINGS, wire::TextRuleParts { max: source.max, fields: field_rules(&source.fields)? })
        .map_err(|error| format!("report rule: {error:?}"))
}

fn default_title() -> String {
    "title".into()
}

/// Read bounded global settings and a workspace override, replacing only
/// keys named by the latter. Objects merge recursively; lists replace whole.
pub fn read(global: &Path, workspace: Option<&Path>) -> Result<Settings, String> {
    let mut document = read_document(global)?;
    if let Some(path) = workspace {
        let override_document = read_document(path)?;
        merge(&mut document, override_document);
    }
    let settings: Settings = serde_json::from_value(document).map_err(|error| format!("local settings: {error}"))?;
    if settings.chat.is_empty()
        || settings.chat == "."
        || settings.chat == ".."
        || settings.chat.contains(['/', '\\', '\0'])
        || settings.models.is_empty()
        || settings
            .models
            .iter()
            .any(|model| model.endpoint.is_empty() || model.name.is_empty() || model.price_unit == 0)
        || settings.budget.turns == 0
        || settings.budget.seconds == 0
        || settings.waiting_seconds == 0
        || settings.title_field.is_empty()
    {
        return Err("local settings contain an empty or zero required value".into());
    }
    if !settings.agent.is_object() {
        return Err("local agent configuration must be an object".into());
    }
    Ok(settings)
}

fn read_document(path: &Path) -> Result<Value, String> {
    let size = fs::metadata(path).map_err(|error| format!("settings metadata {}: {error}", path.display()))?.len();
    if size > SETTINGS_BYTES {
        return Err(format!("settings {} exceed {SETTINGS_BYTES} bytes", path.display()));
    }
    let bytes = fs::read(path).map_err(|error| format!("settings read {}: {error}", path.display()))?;
    if bytes.len() as u64 > SETTINGS_BYTES {
        return Err(format!("settings {} exceed {SETTINGS_BYTES} bytes", path.display()));
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("settings JSON {}: {error}", path.display()))
}

fn merge(base: &mut Value, override_value: Value) {
    match (base, override_value) {
        (Value::Object(base), Value::Object(overrides)) => {
            for (name, replacement) in overrides {
                match base.get_mut(&name) {
                    Some(current) => merge(current, replacement),
                    None => {
                        base.insert(name, replacement);
                    }
                }
            }
        }
        (base, replacement) => *base = replacement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_replaces_only_its_named_settings_and_agent_fields() {
        let mut document = serde_json::json!({
            "agent": {"profile":"standard", "memory_bytes":42, "environment":[]},
            "chat":"one", "instructions":"global", "models":[{"endpoint":"main","name":"small","max_tokens":32,
                "input_price":1,"cached_price":1,"output_price":1,"price_unit":1}],
            "budget":{"turns":8,"spend":100,"seconds":60}, "waiting_seconds":30
        });
        merge(&mut document, serde_json::json!({"instructions":"workspace", "agent":{"memory_bytes":99}}));
        let settings: Settings = serde_json::from_value(document).expect("merged settings");
        assert_eq!(settings.instructions, "workspace");
        assert_eq!(settings.agent["profile"], "standard");
        assert_eq!(settings.agent["memory_bytes"], 99);
        assert_eq!(settings.models[0].name, "small");
    }

    #[test]
    fn invalid_model_and_zero_budget_are_refused_before_a_chat_opens() {
        let path = std::env::temp_dir().join(format!("smith-local-settings-{}", std::process::id()));
        let document = serde_json::json!({"agent":{},"chat":"one","instructions":"x",
            "models":[{"endpoint":"","name":"small","max_tokens":32,"input_price":1,"cached_price":1,
                "output_price":1,"price_unit":1}],"budget":{"turns":0,"spend":1,"seconds":60},"waiting_seconds":30});
        fs::write(&path, serde_json::to_vec(&document).expect("JSON")).expect("settings file");
        assert!(read(&path, None).is_err());
        fs::remove_file(path).expect("remove settings file");
    }

    #[test]
    fn priced_local_policy_round_trips_through_the_child_charter() {
        let mut entries = List::with_capacity(1);
        entries
            .push(channel::Endpoint { name: Box::from(&b"main"[..]), number: 7, dialect: 1, account: 0 })
            .expect("one endpoint");
        let endpoints = channel::Endpoints::new(entries);
        let settings = Settings {
            agent: serde_json::json!({}),
            chat: "one".into(),
            instructions: "Assist".into(),
            models: vec![Model {
                endpoint: "main".into(),
                name: "small".into(),
                max_tokens: 32,
                input_price: 1,
                cached_price: 1,
                output_price: 1,
                price_unit: 1,
            }],
            budget: Budget { turns: 8, spend: 1, seconds: 60 },
            waiting_seconds: 30,
            brief: vec![],
            directories: vec![],
            conventions: None,
            contract: None,
            deliver: None,
            title_field: "title".into(),
        };
        let prepared = policy(&settings, &endpoints, crate::limits::LIMITS).expect("bounded policy");
        assert!(prepared.paths.is_empty());
        assert_eq!(prepared.limits.endpoints.as_ref(), &[run::charter::Endpoint(7)]);
        let encoded = charter(&prepared.config, &endpoints).expect("matching charter");
        assert!(!encoded.is_empty());
    }
}
