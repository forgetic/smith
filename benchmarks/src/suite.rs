//! Suites retain task selections, pins and budgets, never live attempt state.
//! `validate_suite` resolves references; `choose_guards` selects affordable
//! guarded tasks from estimates until summaries supply medians
//! (benchmarks.md, sections 4.2 and 5.3).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Agent, Kind, ModelTiers, Provider, Refusal, Task, check_task, read_configuration};

/// The attempt ordering requested by the suite author.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Design {
    /// Each task runs once per repetition and configuration.
    Single,
    /// Each block runs all arms in an order drawn from its seed.
    Interleaved,
}

/// A catalogue selector supplied by the suite author.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub kind: Kind,
    #[serde(default)]
    pub behaviours: Vec<String>,
}

/// A pinned agent configuration named by the suite author.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SuiteAgent {
    pub agent: Agent,
    pub provider: Provider,
    pub config: String,
}

/// A comparison arm supplied by the suite author to the runner.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "source", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Arm {
    /// A smith binary built from a recorded commit.
    Binary { name: String, commit: String },
    /// A smith settings override compared with the suite's base settings.
    Override { name: String, smith: BTreeMap<String, toml::Value> },
    /// Another pinned agent configuration compared on the same task.
    Agent { name: String, agent: Agent, provider: Provider, config: String },
}

/// A committed run specification supplied by its author to the runner.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Suite {
    pub name: String,
    pub tier: String,
    pub design: Design,
    pub repetitions: u32,
    /// Zero or one rerun after a failed attempt.
    pub rerun_failed: u32,
    pub max_wall_seconds: u64,
    pub max_tokens: u64,
    #[serde(default)]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub select: Vec<Selection>,
    pub agents: Vec<SuiteAgent>,
    #[serde(default, rename = "arm")]
    pub arms: Vec<Arm>,
}

/// A validated task and its catalogue name, supplied to suite resolution.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogueTask {
    pub name: String,
    pub manifest: PathBuf,
    pub task: Task,
}

/// Affordable guarded task names and the tasks omitted from the budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardSelection {
    pub selected: Vec<String>,
    pub omitted: Vec<(String, String)>,
    pub estimated_seconds: u64,
    pub estimated_tokens: u64,
}

/// Read a suite strictly; resolving its references is a separate offline check.
pub fn read_suite(file: &Path) -> Result<Suite, Refusal> {
    crate::formats::read_document(file)
}

/// Read frozen tasks from a catalogue and give each its kind/id name.
pub fn catalogue(tasks: &Path, design: &Path) -> Result<Vec<CatalogueTask>, Refusal> {
    let mut manifests = Vec::new();
    crate::check::find_manifests(tasks, &mut manifests)?;
    manifests.sort();
    let mut entries = Vec::new();
    let mut names = BTreeSet::new();
    for manifest in manifests {
        let task = check_task(&manifest, design)?;
        let kind = match task.kind {
            Kind::Probe => "probes",
            Kind::Fixture => "fixtures",
            Kind::Repository => "repository",
        };
        let name = format!("{kind}/{}", task.id);
        if !names.insert(name.clone()) {
            return Err(Refusal::new(&manifest, "id", format!("duplicate catalogue task {name}")));
        }
        entries.push(CatalogueTask { name, manifest, task });
    }
    Ok(entries)
}

/// Validate a suite's quantities, task names, selectors, model tiers and pins.
pub fn validate_suite(
    file: &Path,
    suite: &Suite,
    tasks: &[CatalogueTask],
    agents: &Path,
    models: &ModelTiers,
) -> Result<Vec<String>, Refusal> {
    if suite.name.is_empty()
        || suite.repetitions == 0
        || suite.rerun_failed > 1
        || suite.max_wall_seconds == 0
        || suite.max_tokens == 0
        || suite.agents.is_empty()
    {
        return Err(Refusal::new(
            file,
            "suite",
            "name, agents and positive budgets/repetitions are required; at most one failure rerun",
        ));
    }
    let mut selected = BTreeSet::new();
    for name in &suite.tasks {
        if !task_exists(name, tasks) {
            return Err(Refusal::new(file, "tasks", format!("unknown task or variant {name}")));
        }
        selected.insert(name.clone());
    }
    for selector in &suite.select {
        let matches: Vec<&CatalogueTask> = tasks
            .iter()
            .filter(|entry| {
                entry.task.kind == selector.kind
                    && selector.behaviours.iter().all(|behaviour| entry.task.behaviours.contains(behaviour))
            })
            .collect();
        if matches.is_empty() {
            return Err(Refusal::new(file, "select", "selector names no existing task"));
        }
        for task in matches {
            selected.insert(task.name.clone());
        }
    }
    if selected.is_empty() {
        return Err(Refusal::new(file, "tasks/select", "suite must name an existing task"));
    }
    for agent in &suite.agents {
        validate_agent(file, agent, &suite.tier, agents, models)?;
    }
    let mut arm_names = BTreeSet::new();
    for arm in &suite.arms {
        let name = match arm {
            Arm::Binary { name, commit } => {
                if commit.is_empty() {
                    return Err(Refusal::new(file, "arm.commit", "binary arm needs a commit"));
                }
                name
            }
            Arm::Override { name, smith: _ } => name,
            Arm::Agent { name, agent, provider, config } => {
                validate_agent(
                    file,
                    &SuiteAgent { agent: *agent, provider: *provider, config: config.clone() },
                    &suite.tier,
                    agents,
                    models,
                )?;
                name
            }
        };
        if name.is_empty() || !arm_names.insert(name) {
            return Err(Refusal::new(file, "arm.name", "arm names must be nonempty and unique"));
        }
    }
    Ok(selected.into_iter().collect())
}

fn validate_agent(
    file: &Path,
    agent: &SuiteAgent,
    tier: &str,
    agents: &Path,
    models: &ModelTiers,
) -> Result<(), Refusal> {
    crate::check::check_path(file, "agents.config", Path::new(&agent.config))?;
    if agent.config.contains('/') {
        return Err(Refusal::new(file, "agents.config", "configuration is a name within the agent directory"));
    }
    let pin_file = agents.join(agent.agent.directory()).join(format!("{}.pin.toml", agent.config));
    let pin = read_configuration(&pin_file)?;
    if pin.pin.agent != agent.agent || pin.pin.provider != agent.provider || pin.pin.name != agent.config {
        return Err(Refusal::new(file, "agents", "suite agent/provider/name disagrees with its configuration pin"));
    }
    models.lookup(&agents.join("models.toml"), tier, agent.provider)?;
    Ok(())
}

fn task_exists(name: &str, tasks: &[CatalogueTask]) -> bool {
    tasks.iter().any(|entry| {
        if name == entry.name {
            return true;
        }
        let Some(variant) = name.strip_prefix(&format!("{}/", entry.name)) else {
            return false;
        };
        entry.task.variant.iter().any(|entry| entry.name == variant)
    })
}

/// Select guarded tasks cheapest first within the suite's aggregate budgets.
/// Missing estimates are omitted with a reason, never treated as zero cost.
#[must_use]
pub fn choose_guards(suite: &Suite, tasks: &[CatalogueTask], sections: &[String]) -> GuardSelection {
    let mut candidates = Vec::new();
    let mut selection =
        GuardSelection { selected: Vec::new(), omitted: Vec::new(), estimated_seconds: 0, estimated_tokens: 0 };
    let arms = u64::try_from(suite.arms.len().max(1)).expect("arm count fits u64");
    let agents = u64::try_from(suite.agents.len()).expect("agent count fits u64");
    for entry in tasks {
        if !entry.task.guards.iter().any(|guard| sections.contains(guard)) {
            continue;
        }
        let Some(estimate) = &entry.task.estimate else {
            selection.omitted.push((entry.name.clone(), "cost unavailable".into()));
            continue;
        };
        let repetitions = u64::from(entry.task.repetitions.unwrap_or(suite.repetitions));
        let multiplier = repetitions
            .checked_mul(u64::from(suite.rerun_failed) + 1)
            .and_then(|count| count.checked_mul(agents))
            .and_then(|count| count.checked_mul(arms));
        let cost = multiplier
            .and_then(|count| Some((estimate.seconds.checked_mul(count)?, estimate.tokens.checked_mul(count)?)));
        if let Some((seconds, tokens)) = cost {
            candidates.push((seconds, tokens, entry.name.clone()));
            for variant in &entry.task.variant {
                candidates.push((seconds, tokens, format!("{}/{}", entry.name, variant.name)));
            }
        } else {
            selection.omitted.push((entry.name.clone(), "cost overflow".into()));
        }
    }
    candidates.sort();
    for (seconds, tokens, name) in candidates {
        let wall = selection.estimated_seconds.checked_add(seconds);
        let tokens = selection.estimated_tokens.checked_add(tokens);
        if let Some(wall) = wall
            && let Some(tokens) = tokens
            && wall <= suite.max_wall_seconds
            && tokens <= suite.max_tokens
        {
            selection.selected.push(name);
            selection.estimated_seconds = wall;
            selection.estimated_tokens = tokens;
        } else {
            selection.omitted.push((name, "suite wall or token budget".into()));
        }
    }
    selection
}
