//! Host-supplied result contracts and protocol-decoded finish values
//! (domain/run.md, sections 3.1, 7.1–7.3). Labels have no built-in
//! meaning: this module keeps no runtime state, reads no clocks and performs
//! no checkout or host effect. The run admits a satisfiable bounded contract,
//! bounds a declaration with `owned_bytes`, then calls `judge` for shape.
//! `Report`, verdict and declared failure finish without delivery; a change
//! follows exclusive writable checks and the generic host terminal.

use alloc::boxed::Box;
use core::mem::size_of;

use skein_lib::{List, bytes::copy_of};

use crate::charter::{count, len};
use crate::limits::Limits;

/// The host's permitted forms. Admission requires at least one, valid unique rules and a smallest accepted value for every allowed form within the result cap.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct OutcomeSpec {
    /// `Change` contract, or none. A validated change requires the existing host `Delivery` terminal before acceptance.
    pub change: Option<ChangeSpec>,
    /// Closed host labels and their contracts; empty forbids verdicts. Count is bounded by `Limits`.verdicts, total rule ownership by `Limits`.`run_bytes`.
    pub verdicts: Box<[VerdictRule]>,
    /// `Report` contract, or none. A validated report ends after all sessions settle, without checks or `Delivery`.
    pub report: Option<TextSpec>,
    /// Declared-failure contract, or none. An admitted reason is an accepted result, distinct from a runtime failure.
    pub failure: Option<TextSpec>,
}

/// One required field chosen by the host; the run compares its name exactly and applies its byte bound.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct FieldRule {
    /// Nonempty required byte name, unique within this rule list. Box and name count against `Limits`.`run_bytes`.
    pub name: Box<[u8]>,
    /// Positive largest accepted value byte length. The required value must be present exactly once and nonempty; all fields also share the aggregate outcome cap.
    pub max: u32,
}

/// Host field contract for a final Change or separately granted mid-run delivery.
/// The result contract decides whether failing checks block delivery.
/// Minimum field/container storage is checked at charter admission.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ChangeSpec {
    /// Whether a failing workspace check blocks delivery. A mid-run delivery
    /// without a Change result contract always requires passing checks.
    pub checks_must_pass: bool,

    /// Required result fields with host-chosen byte names and individual value bounds; no title/body vocabulary is interpreted by smith.
    pub fields: Box<[FieldRule]>,
}

/// The host's report or declared-failure contract. Its text or reason has a
/// maximum byte length; required fields are nonempty and bounded.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct TextSpec {
    /// Inclusive largest report text or declared failure reason length in bytes; aggregate ownership still obeys `Limits`.`outcome_bytes`.
    pub max: u32,
    /// Required host-named result fields; retained rules count against `Limits`.`run_bytes`.
    pub fields: Box<[FieldRule]>,
}

/// One host-chosen label and its text, root fields and allowed result items. Label equality is byte-exact; smith assigns no review semantics.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct VerdictRule {
    /// Nonempty host label, unique across the closed verdict list; counted in charter ownership.
    pub name: Box<[u8]>,
    /// Largest verdict text byte length; zero permits empty text only. `Verdict` text is otherwise allowed to be empty.
    pub text_max: u32,
    /// Required fields on the verdict itself, checked independently of item fields.
    pub fields: Box<[FieldRule]>,
    /// Host-selected count range and per-kind field requirements; no items are required when min is zero.
    pub items: ItemSpec,
}

/// Host contract for the result's ordered items. Admission requires min <= max and a nonempty kind list when max is positive; the minimum accepted result must fit aggregate ownership.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ItemSpec {
    /// Inclusive smallest accepted item count; its smallest admissible items must fit `Limits`.`outcome_bytes` together with the result.
    pub min: u32,
    /// Inclusive largest accepted item count; concrete item storage and payload remain bounded by the aggregate outcome cap.
    pub max: u32,
    /// Closed, uniquely named item kinds. Each carries its own required fields, and every rule contributes to charter ownership.
    pub kinds: Box<[ItemRule]>,
}

/// One host-named allowed item kind, with field requirements specific to that kind.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ItemRule {
    /// Nonempty byte name, unique in the kind list. The run never interprets its meaning.
    pub kind: Box<[u8]>,
    /// Required fields on an item of this exact kind; another kind's fields confer no requirement.
    pub fields: Box<[FieldRule]>,
}

/// Protocol-decoded LLM change result. The run validates fields and aggregate ownership before checks/`Delivery`; the host receives these fields unchanged.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Change {
    /// LLM-provided named values. Host rules require nonempty bounded values for their names; extra names are permitted. All names must be unique.
    pub fields: Box<[Field]>,
}

impl Change {
    /// Count every owned field container, name and value, including unknown extra
    /// fields. The run/root check this aggregate before retaining a delivery ask.
    #[must_use]
    pub fn owned_bytes(&self) -> Option<u64> {
        fields_cost(&self.fields)
    }
}

/// Protocol-decoded LLM report. Accepted after contract judgement and settlement, without checking or pushing the workspace.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Report {
    /// Report text within the host `TextSpec` maximum; its bytes count against aggregate ownership.
    pub text: Box<[u8]>,
    /// LLM-provided result fields, checked against the report's own required rules and aggregate ownership.
    pub fields: Box<[Field]>,
}

/// Protocol-decoded LLM declaration that the work cannot be completed. Its successful contract judgement yields an accepted result, never a runtime failure classification.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeclaredFailure {
    /// LLM explanation within the host `TextSpec` maximum; the host interprets its meaning.
    pub reason: Box<[u8]>,
    /// LLM-provided result fields, checked against this failure contract's required rules and aggregate ownership.
    pub fields: Box<[Field]>,
}

/// Protocol-decoded LLM verdict. The closed host label selects text, root-field and per-kind item requirements.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Verdict {
    /// Byte-exact host verdict label; an unknown label is rejected as finish feedback.
    pub name: Box<[u8]>,
    /// LLM verdict explanation, at most `VerdictRule::text_max` bytes; empty is permitted.
    pub text: Box<[u8]>,
    /// Named values on the verdict itself; independent of every item's field list.
    pub fields: Box<[Field]>,
    /// Ordered result items. Count, kind-specific required fields and aggregate boxed-storage-plus-payload ownership are checked before acceptance.
    pub items: Box<[Item]>,
}

/// One LLM-provided item whose byte kind selects its host-defined field rules. It starts no task or action on its own.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Item {
    /// Exact allowed host kind name, counted with item storage against the aggregate outcome cap.
    pub kind: Box<[u8]>,
    /// LLM-provided named values, unique within this item and validated under this kind's required rules.
    pub fields: Box<[Field]>,
}

/// One LLM-provided named result value. Required host names must exist, be nonempty and fit their own cap; extra fields may be empty, but duplicates are always refused.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Field {
    /// Byte name interpreted only as a contract label; box storage and name bytes count against `Limits`.`outcome_bytes`.
    pub name: Box<[u8]>,
    /// Result value bytes interpreted by the host. Required names obey `FieldRule::max`; extra fields remain bounded by aggregate ownership.
    pub value: Box<[u8]>,
}

/// Bounded typed finish feedback returned to the LLM; it does not end the run and carries no authority.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Problems {
    /// First at most `LISTED` problems in deterministic validation order. Copied field names come from the already bounded contract or declaration.
    pub listed: Box<[Problem]>,
    /// Saturating count of additional problems omitted after the feedback list fills.
    pub more: u32,
}

/// Protocol-decoded finish form. All payloads share `Limits.outcome_bytes`
/// checked by the run before shape judgement. Only `Change` requires `Delivery`;
/// every other valid form winds down the sessions and becomes the accepted answer.
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum Declared {
    /// Workspace change with generic host fields; accepts only after host delivery.
    Change(
        /// Protocol-decoded fields validated before any check or `Delivery`.
        Change,
    ),
    /// Host-labelled verdict; starts no workspace delivery.
    Verdict(
        /// Protocol-decoded verdict and per-kind items, bounded before judgement.
        Verdict,
    ),
    /// Text report satisfying the host's own report fields.
    Report(
        /// Protocol-decoded text and fields, accepted without `Delivery`.
        Report,
    ),
    /// LLM-declared inability to complete, distinct from runtime failure.
    Failure(
        /// Protocol-decoded bounded reason and fields, accepted after judgement.
        DeclaredFailure,
    ),
}

/// Form whose text failed its host contract; it contains no user content.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Form {
    /// `Report` text.
    Report,
    /// `Verdict` explanation.
    Verdict,
    /// Declared failure reason.
    Failure,
}

/// One shape or ownership violation returned as finish feedback. `Item` indices
/// are zero-based and `None` on fields of the result itself. Names are copied
/// only into the first eight listed problems, from already bounded inputs.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Problem {
    /// Aggregate boxed-storage-plus-payload bytes exceed the run's cap.
    TooLarge {
        /// `Limits.outcome_bytes` applied before shape judgement.
        max: u64,
    },
    /// Host forbids change.
    ChangeNotAllowed,
    /// Host forbids verdict.
    VerdictNotAllowed,
    /// Host forbids report.
    ReportNotAllowed,
    /// Host forbids declared failure.
    FailureNotAllowed,
    /// No byte-exact host verdict label matches.
    UnknownVerdict,
    /// Result text exceeds its host-defined individual byte cap.
    TextTooLarge {
        /// The text-bearing form whose byte length exceeded the cap.
        form: Form,
        /// Inclusive host text/reason byte limit.
        max: u32,
    },
    /// `Verdict` has fewer items than required.
    TooFewItems {
        /// Inclusive host minimum item count.
        min: u32,
    },
    /// `Verdict` has more items than allowed.
    TooManyItems {
        /// Inclusive host maximum item count.
        max: u32,
    },
    /// `Item` kind is not in this verdict's closed list.
    KindNotAllowed {
        /// Zero-based index in the declared verdict's ordered items.
        item: u32,
    },
    /// A required named field is absent.
    MissingField {
        /// `None` for the result itself; otherwise its zero-based item index.
        item: Option<u32>,
        /// Exact required or repeated byte name; copied only while feedback room remains.
        field: Box<[u8]>,
    },
    /// A required named value is empty.
    EmptyField {
        /// `None` for the result itself; otherwise its zero-based item index.
        item: Option<u32>,
        /// Exact required or repeated byte name; copied only while feedback room remains.
        field: Box<[u8]>,
    },
    /// The same byte field name occurs more than once.
    RepeatedField {
        /// `None` for the result itself; otherwise its zero-based item index.
        item: Option<u32>,
        /// Exact required or repeated byte name; copied only while feedback room remains.
        field: Box<[u8]>,
    },
    /// A required named value exceeds its individual host cap.
    FieldTooLarge {
        /// `None` for the result itself; otherwise its zero-based item index.
        item: Option<u32>,
        /// Exact required or repeated byte name; copied only while feedback room remains.
        field: Box<[u8]>,
        /// Inclusive maximum required value byte length supplied by the host.
        max: u32,
    },
}

impl Problems {
    /// Maximum retained feedback problems; additional problems are counted.
    pub const LISTED: u32 = 8;
}

/// Pure deterministic shape judgement of decoded finish against the host
/// contract. Returns success or bounded typed feedback, without any effect.
/// It checks individual field/text caps and item rules, not the aggregate
/// ownership cap: the run first calls `owned_bytes` and checks its limit.
/// Other callers must supply their own aggregate input bound.
pub fn judge(spec: &OutcomeSpec, declared: &Declared) -> Result<(), Problems> {
    let mut found = Found { listed: List::with_capacity(Problems::LISTED), more: 0 };
    match declared {
        Declared::Change(change) => match &spec.change {
            Some(rule) => judge_fields(&rule.fields, &change.fields, None, &mut found),
            None => found.add(Problem::ChangeNotAllowed),
        },
        Declared::Report(report) => match &spec.report {
            Some(rule) => judge_text(rule, &report.text, &report.fields, Form::Report, &mut found),
            None => found.add(Problem::ReportNotAllowed),
        },
        Declared::Failure(failure) => match &spec.failure {
            Some(rule) => judge_text(rule, &failure.reason, &failure.fields, Form::Failure, &mut found),
            None => found.add(Problem::FailureNotAllowed),
        },
        Declared::Verdict(verdict) => judge_verdict(&spec.verdicts, verdict, &mut found),
    }
    if found.listed.is_empty() {
        return Ok(());
    }
    Err(Problems { listed: found.listed.into_boxed(), more: found.more })
}

struct Found {
    listed: List<Problem>,
    more: u32,
}

impl Found {
    fn add(&mut self, problem: Problem) {
        match self.listed.push(problem) {
            Ok(()) => {}
            Err(_) => self.more = self.more.saturating_add(1),
        }
    }

    fn field(&mut self, item: Option<u32>, field: &[u8], why: FieldProblem) {
        if self.listed.room() == 0 {
            self.more = self.more.saturating_add(1);
            return;
        }
        let field = copy_of(field);
        self.add(match why {
            FieldProblem::Missing => Problem::MissingField { item, field },
            FieldProblem::Empty => Problem::EmptyField { item, field },
            FieldProblem::Repeated => Problem::RepeatedField { item, field },
            FieldProblem::TooLarge { max } => Problem::FieldTooLarge { item, field, max },
        });
    }
}

#[derive(Clone, Copy)]
enum FieldProblem {
    Missing,
    Empty,
    Repeated,
    TooLarge { max: u32 },
}

pub(crate) fn judge_change(spec: &ChangeSpec, change: &Change) -> Result<(), Problems> {
    let mut found = Found { listed: List::with_capacity(Problems::LISTED), more: 0 };
    judge_fields(&spec.fields, &change.fields, None, &mut found);
    if found.listed.is_empty() { Ok(()) } else { Err(Problems { listed: found.listed.into_boxed(), more: found.more }) }
}

pub(crate) fn valid_change(spec: &ChangeSpec, limits: &Limits) -> bool {
    valid_fields(&spec.fields) && fits(min_fields(&spec.fields), limits.outcome_bytes)
}

pub(crate) fn change_cost(spec: &ChangeSpec) -> Option<u64> {
    rule_fields_cost(&spec.fields)
}

pub(crate) fn change_bytes(change: &Change) -> Option<u64> {
    fields_cost(&change.fields)
}

fn judge_fields(rules: &[FieldRule], fields: &[Field], item: Option<u32>, found: &mut Found) {
    for rule in rules {
        let given = match field_named(fields, &rule.name) {
            Some(index) => fields.get(index),
            None => None,
        };
        match given {
            None => found.field(item, &rule.name, FieldProblem::Missing),
            Some(field) => {
                if field.value.is_empty() {
                    found.field(item, &rule.name, FieldProblem::Empty);
                }
                if past(field.value.len(), rule.max) {
                    found.field(item, &rule.name, FieldProblem::TooLarge { max: rule.max });
                }
            }
        }
    }
    for (at, field) in fields.iter().enumerate() {
        let earlier = fields.get(..at).unwrap_or_default();
        let later = fields.get(at.saturating_add(1)..).unwrap_or_default();
        if field_named(earlier, &field.name).is_none() && field_named(later, &field.name).is_some() {
            found.field(item, &field.name, FieldProblem::Repeated);
        }
    }
}

fn field_named(fields: &[Field], name: &[u8]) -> Option<usize> {
    for (index, field) in fields.iter().enumerate() {
        if &*field.name == name {
            return Some(index);
        }
    }
    None
}

fn judge_text(rule: &TextSpec, text: &[u8], fields: &[Field], form: Form, found: &mut Found) {
    if past(text.len(), rule.max) {
        found.add(Problem::TextTooLarge { form, max: rule.max });
    }
    judge_fields(&rule.fields, fields, None, found);
}

fn judge_verdict(rules: &[VerdictRule], verdict: &Verdict, found: &mut Found) {
    if rules.is_empty() {
        found.add(Problem::VerdictNotAllowed);
        return;
    }
    let mut named = None;
    for rule in rules {
        if rule.name == verdict.name {
            named = Some(rule);
            break;
        }
    }
    let Some(rule) = named else {
        found.add(Problem::UnknownVerdict);
        return;
    };
    if past(verdict.text.len(), rule.text_max) {
        found.add(Problem::TextTooLarge { form: Form::Verdict, max: rule.text_max });
    }
    judge_fields(&rule.fields, &verdict.fields, None, found);
    if count(verdict.items.len()) < rule.items.min {
        found.add(Problem::TooFewItems { min: rule.items.min });
    }
    if past(verdict.items.len(), rule.items.max) {
        found.add(Problem::TooManyItems { max: rule.items.max });
    }
    for (index, item) in verdict.items.iter().enumerate() {
        let mut kind = None;
        for allowed in &rule.items.kinds {
            if allowed.kind == item.kind {
                kind = Some(allowed);
                break;
            }
        }
        let index = count(index);
        match kind {
            Some(allowed) => judge_fields(&allowed.fields, &item.fields, Some(index), found),
            None => found.add(Problem::KindNotAllowed { item: index }),
        }
    }
}

/// Admission shape and checked minimum-fit validation, without effects.
/// Charter ownership is bounded separately by charter admission.
pub(crate) fn is_valid(spec: &OutcomeSpec, limits: &Limits) -> bool {
    if spec.change.is_none() && spec.verdicts.is_empty() && spec.report.is_none() && spec.failure.is_none() {
        return false;
    }
    if count(spec.verdicts.len()) > limits.verdicts {
        return false;
    }
    if let Some(rule) = &spec.change
        && (!valid_fields(&rule.fields) || !fits(min_fields(&rule.fields), limits.outcome_bytes))
    {
        return false;
    }
    for rule in [&spec.report, &spec.failure].into_iter().flatten() {
        if !valid_fields(&rule.fields) || !fits(min_fields(&rule.fields), limits.outcome_bytes) {
            return false;
        }
    }
    for (at, rule) in spec.verdicts.iter().enumerate() {
        if rule.name.is_empty() || !valid_fields(&rule.fields) || rule.items.min > rule.items.max {
            return false;
        }
        for other in spec.verdicts.get(at.saturating_add(1)..).unwrap_or_default() {
            if other.name == rule.name {
                return false;
            }
        }
        if rule.items.max > 0 && rule.items.kinds.is_empty() {
            return false;
        }
        for (index, kind) in rule.items.kinds.iter().enumerate() {
            if kind.kind.is_empty() || !valid_fields(&kind.fields) {
                return false;
            }
            for other in rule.items.kinds.get(index.saturating_add(1)..).unwrap_or_default() {
                if other.kind == kind.kind {
                    return false;
                }
            }
        }
        if !fits(min_verdict(rule), limits.outcome_bytes) {
            return false;
        }
    }
    true
}

fn fits(cost: Option<u64>, max: u64) -> bool {
    match cost {
        Some(cost) => cost <= max,
        None => false,
    }
}

fn valid_fields(rules: &[FieldRule]) -> bool {
    for (at, rule) in rules.iter().enumerate() {
        if rule.name.is_empty() || rule.max == 0 {
            return false;
        }
        for other in rules.get(at.saturating_add(1)..).unwrap_or_default() {
            if other.name == rule.name {
                return false;
            }
        }
    }
    true
}

fn min_fields(rules: &[FieldRule]) -> Option<u64> {
    let fixed = u64::try_from(size_of::<Field>()).ok()?;
    let mut cost: u64 = 0;
    for rule in rules {
        cost = cost.checked_add(fixed)?.checked_add(len(&rule.name)?)?.checked_add(1)?;
    }
    Some(cost)
}

fn min_verdict(rule: &VerdictRule) -> Option<u64> {
    let base = len(&rule.name)?.checked_add(min_fields(&rule.fields)?)?;
    if rule.items.min == 0 {
        return Some(base);
    }
    let fixed = u64::try_from(size_of::<Item>()).ok()?;
    let mut smallest = None;
    for kind in &rule.items.kinds {
        let cost = fixed.checked_add(len(&kind.kind)?)?.checked_add(min_fields(&kind.fields)?)?;
        smallest = Some(match smallest {
            None => cost,
            Some(previous) => cost.min(previous),
        });
    }
    let cost = smallest?;
    base.checked_add(cost.checked_mul(u64::from(rule.items.min))?)
}

/// Checked owned rule containers and names beyond the inline charter.
/// Overflow refuses charter admission rather than truncating its charge.
pub(crate) fn cost(spec: &OutcomeSpec) -> Option<u64> {
    let mut cost: u64 = 0;
    if let Some(rule) = &spec.change {
        cost = cost.checked_add(rule_fields_cost(&rule.fields)?)?;
    }
    for rule in [&spec.report, &spec.failure].into_iter().flatten() {
        cost = cost.checked_add(rule_fields_cost(&rule.fields)?)?;
    }
    let verdict = u64::try_from(size_of::<VerdictRule>()).ok()?;
    let kind = u64::try_from(size_of::<ItemRule>()).ok()?;
    for rule in &spec.verdicts {
        cost =
            cost.checked_add(verdict)?.checked_add(len(&rule.name)?)?.checked_add(rule_fields_cost(&rule.fields)?)?;
        for item in &rule.items.kinds {
            cost =
                cost.checked_add(kind)?.checked_add(len(&item.kind)?)?.checked_add(rule_fields_cost(&item.fields)?)?;
        }
    }
    Some(cost)
}

fn rule_fields_cost(rules: &[FieldRule]) -> Option<u64> {
    let fixed = u64::try_from(size_of::<FieldRule>()).ok()?;
    let mut cost: u64 = 0;
    for rule in rules {
        cost = cost.checked_add(fixed)?.checked_add(len(&rule.name)?)?;
    }
    Some(cost)
}

/// Checked boxed-storage-plus-payload bytes retained beyond the inline
/// declaration. Counts every result/item field, kind/label and text exactly
/// once, excluding allocator overhead and the inline enum's fixed size.
/// The run and its root both use this value for their respective admission
/// and ownership checks. Overflow yields `None`, never an accepted charge.
#[must_use]
pub fn owned_bytes(declared: &Declared) -> Option<u64> {
    match declared {
        Declared::Change(change) => fields_cost(&change.fields),
        Declared::Report(report) => len(&report.text)?.checked_add(fields_cost(&report.fields)?),
        Declared::Failure(failure) => len(&failure.reason)?.checked_add(fields_cost(&failure.fields)?),
        Declared::Verdict(verdict) => {
            let fixed = u64::try_from(size_of::<Item>()).ok()?;
            let mut cost =
                len(&verdict.name)?.checked_add(len(&verdict.text)?)?.checked_add(fields_cost(&verdict.fields)?)?;
            for item in &verdict.items {
                cost =
                    cost.checked_add(fixed)?.checked_add(len(&item.kind)?)?.checked_add(fields_cost(&item.fields)?)?;
            }
            Some(cost)
        }
    }
}

fn fields_cost(fields: &[Field]) -> Option<u64> {
    let fixed = u64::try_from(size_of::<Field>()).ok()?;
    let mut cost: u64 = 0;
    for field in fields {
        cost = cost.checked_add(fixed)?.checked_add(len(&field.name)?)?.checked_add(len(&field.value)?)?;
    }
    Some(cost)
}

/// One bounded aggregate-ownership refusal for the supplied run byte cap.
pub(crate) fn too_large(max: u64) -> Problems {
    Problems { listed: Box::new([Problem::TooLarge { max }]), more: 0 }
}

fn past(length: usize, max: u32) -> bool {
    match u32::try_from(length) {
        Ok(length) => length > max,
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Change, ChangeSpec, Declared, DeclaredFailure, Field, FieldRule, Form, Item, ItemRule, ItemSpec, Limits, List,
        OutcomeSpec, Problem, Report, TextSpec, Verdict, VerdictRule, is_valid, judge, owned_bytes,
    };
    use crate::tests::LIMITS;
    use alloc::boxed::Box;
    use core::mem::size_of;

    fn rules(name: &[u8], max: u32) -> Box<[FieldRule]> {
        Box::new([FieldRule { name: name.into(), max }])
    }

    fn fields(name: &[u8], value: &[u8]) -> Box<[Field]> {
        Box::new([Field { name: name.into(), value: value.into() }])
    }

    fn specification() -> OutcomeSpec {
        OutcomeSpec {
            change: Some(ChangeSpec { checks_must_pass: true, fields: rules(b"summary", 4) }),
            report: Some(TextSpec { max: 3, fields: rules(b"source", 3) }),
            failure: Some(TextSpec { max: 3, fields: rules(b"cause", 3) }),
            verdicts: Box::new([VerdictRule {
                name: b"assess".as_slice().into(),
                text_max: 3,
                fields: rules(b"scope", 3),
                items: ItemSpec {
                    min: 1,
                    max: 2,
                    kinds: Box::new([
                        ItemRule { kind: b"line".as_slice().into(), fields: rules(b"path", 3) },
                        ItemRule { kind: b"link".as_slice().into(), fields: rules(b"uri", 3) },
                    ]),
                },
            }]),
        }
    }

    fn verdict() -> Verdict {
        Verdict {
            name: b"assess".as_slice().into(),
            text: Box::new([]),
            fields: fields(b"scope", b"all"),
            items: Box::new([Item { kind: b"link".as_slice().into(), fields: fields(b"uri", b"url") }]),
        }
    }

    #[test]
    fn every_form_uses_host_fields_and_text_bounds() {
        let spec = specification();
        let outcomes = [
            Declared::Change(Change { fields: fields(b"summary", b"done") }),
            Declared::Report(Report { text: Box::new([]), fields: fields(b"source", b"ref") }),
            Declared::Failure(DeclaredFailure { reason: b"no".as_slice().into(), fields: fields(b"cause", b"why") }),
            Declared::Verdict(verdict()),
        ];
        for outcome in outcomes {
            assert_eq!(judge(&spec, &outcome), Ok(()), "{outcome:?}");
        }
        let outcome = Declared::Failure(DeclaredFailure { reason: Box::new([]), fields: fields(b"cause", b"why") });
        assert_eq!(judge(&spec, &outcome), Ok(()));
        let outcome = Declared::Report(Report { text: b"long".as_slice().into(), fields: fields(b"source", b"ref") });
        assert_eq!(
            judge(&spec, &outcome).unwrap_err().listed.as_ref(),
            &[Problem::TextTooLarge { form: Form::Report, max: 3 }]
        );
        let mut value = verdict();
        value.text = b"long".as_slice().into();
        assert_eq!(
            judge(&spec, &Declared::Verdict(value)).unwrap_err().listed.as_ref(),
            &[Problem::TextTooLarge { form: Form::Verdict, max: 3 }]
        );
    }

    #[test]
    fn each_item_kind_requires_its_own_fields() {
        let mut value = verdict();
        value.items[0].fields = fields(b"path", b"src");
        assert_eq!(
            judge(&specification(), &Declared::Verdict(value)).unwrap_err().listed.as_ref(),
            &[Problem::MissingField { item: Some(0), field: b"uri".as_slice().into() }]
        );
        let mut value = verdict();
        value.items[0].kind = b"other".as_slice().into();
        assert_eq!(
            judge(&specification(), &Declared::Verdict(value)).unwrap_err().listed.as_ref(),
            &[Problem::KindNotAllowed { item: 0 }]
        );
        let mut value = verdict();
        value.items = Box::new([]);
        assert_eq!(
            judge(&specification(), &Declared::Verdict(value)).unwrap_err().listed.as_ref(),
            &[Problem::TooFewItems { min: 1 }]
        );
    }

    #[test]
    fn missing_empty_duplicate_and_oversized_fields_have_typed_feedback() {
        let spec = specification();
        let values: [(Box<[Field]>, Problem); 4] = [
            (Box::new([]), Problem::MissingField { item: None, field: b"summary".as_slice().into() }),
            (fields(b"summary", b""), Problem::EmptyField { item: None, field: b"summary".as_slice().into() }),
            (
                fields(b"summary", b"large"),
                Problem::FieldTooLarge { item: None, field: b"summary".as_slice().into(), max: 4 },
            ),
            (
                Box::new([
                    Field { name: b"summary".as_slice().into(), value: b"ok".as_slice().into() },
                    Field { name: b"summary".as_slice().into(), value: b"ok".as_slice().into() },
                ]),
                Problem::RepeatedField { item: None, field: b"summary".as_slice().into() },
            ),
        ];
        for (fields, problem) in values {
            assert_eq!(judge(&spec, &Declared::Change(Change { fields })).unwrap_err().listed.as_ref(), &[problem]);
        }
    }

    #[test]
    fn extra_fields_are_allowed_but_all_their_storage_is_priced() {
        let value = Declared::Report(Report {
            text: Box::new([]),
            fields: Box::new([
                Field { name: b"source".as_slice().into(), value: b"ref".as_slice().into() },
                Field { name: Box::new([]), value: Box::new([]) },
                Field { name: b"opaque".as_slice().into(), value: b"extra".as_slice().into() },
            ]),
        });
        assert_eq!(judge(&specification(), &value), Ok(()));
        assert_eq!(owned_bytes(&value), Some(3 * u64::try_from(size_of::<Field>()).unwrap() + 6 + 3 + 6 + 5));
    }

    #[test]
    fn ownership_counts_item_containers_and_every_payload() {
        let value = Declared::Verdict(verdict());
        let fields = 2 * u64::try_from(size_of::<Field>()).unwrap();
        let items = u64::try_from(size_of::<Item>()).unwrap();
        assert_eq!(owned_bytes(&value), Some(fields + items + 6 + 5 + 3 + 4 + 3 + 3));
    }

    #[test]
    fn admission_prices_minimum_text_and_field_storage_at_the_exact_boundary() {
        let spec = OutcomeSpec {
            change: None,
            verdicts: Box::new([]),
            report: Some(TextSpec { max: 4, fields: rules(b"x", 1) }),
            failure: None,
        };
        let min = u64::try_from(size_of::<Field>()).unwrap() + 1 + 1;
        assert!(is_valid(&spec, &Limits { outcome_bytes: min, ..LIMITS }));
        assert!(!is_valid(&spec, &Limits { outcome_bytes: min - 1, ..LIMITS }));
        let empty = OutcomeSpec { report: Some(TextSpec { max: 0, fields: Box::new([]) }), ..spec.clone() };
        assert!(is_valid(&empty, &Limits { outcome_bytes: 0, ..LIMITS }));
        let bounded = OutcomeSpec { report: Some(TextSpec { max: 1, fields: Box::new([]) }), ..spec };
        assert!(is_valid(&bounded, &LIMITS));
    }

    #[test]
    fn admission_prices_minimum_item_storage_and_rejects_malformed_names() {
        let mut spec = specification();
        spec.change = None;
        spec.report = None;
        spec.failure = None;
        let min = 6
            + u64::try_from(size_of::<Field>()).unwrap()
            + 5
            + 1
            + u64::try_from(size_of::<Item>()).unwrap()
            + 4
            + u64::try_from(size_of::<Field>()).unwrap()
            + 3
            + 1;
        assert!(is_valid(&spec, &Limits { outcome_bytes: min, ..LIMITS }));
        assert!(!is_valid(&spec, &Limits { outcome_bytes: min - 1, ..LIMITS }));
        let repeated = spec.verdicts[0].clone();
        spec.verdicts = Box::new([repeated.clone(), repeated]);
        assert!(!is_valid(&spec, &LIMITS));
        let mut spec = specification();
        spec.report.as_mut().unwrap().fields = rules(b"", 1);
        assert!(!is_valid(&spec, &LIMITS));
        let mut spec = specification();
        spec.report.as_mut().unwrap().fields = rules(b"name", 0);
        assert!(!is_valid(&spec, &LIMITS));
    }

    #[test]
    fn feedback_is_bounded_even_with_many_missing_fields() {
        let mut rules = List::with_capacity(20);
        for index in 1..=20_u8 {
            rules.push(FieldRule { name: Box::new([index]), max: 1 }).expect("room for twenty distinct rules");
        }
        let spec = OutcomeSpec {
            change: Some(ChangeSpec { checks_must_pass: true, fields: rules.into_boxed() }),
            verdicts: Box::new([]),
            report: None,
            failure: None,
        };
        let problems = judge(&spec, &Declared::Change(Change { fields: Box::new([]) })).unwrap_err();
        assert_eq!(problems.listed.len(), 8);
        assert_eq!(problems.more, 12);
    }

    #[test]
    fn forbidden_forms_unknown_labels_and_item_count_are_specific_feedback() {
        let empty = OutcomeSpec { change: None, verdicts: Box::new([]), report: None, failure: None };
        let cases = [
            (Declared::Change(Change { fields: Box::new([]) }), Problem::ChangeNotAllowed),
            (Declared::Report(Report { text: Box::new([]), fields: Box::new([]) }), Problem::ReportNotAllowed),
            (
                Declared::Failure(DeclaredFailure { reason: Box::new([]), fields: Box::new([]) }),
                Problem::FailureNotAllowed,
            ),
            (Declared::Verdict(verdict()), Problem::VerdictNotAllowed),
        ];
        for (value, problem) in cases {
            assert_eq!(judge(&empty, &value).unwrap_err().listed.as_ref(), &[problem]);
        }
        let mut value = verdict();
        value.name = b"another".as_slice().into();
        assert_eq!(
            judge(&specification(), &Declared::Verdict(value)).unwrap_err().listed.as_ref(),
            &[Problem::UnknownVerdict]
        );
        let mut value = verdict();
        let item = value.items[0].clone();
        value.items = Box::new([item.clone(), item.clone(), item]);
        assert_eq!(
            judge(&specification(), &Declared::Verdict(value)).unwrap_err().listed.as_ref(),
            &[Problem::TooManyItems { max: 2 }]
        );
    }

    #[test]
    fn invalid_kind_ranges_and_duplicate_declared_rules_are_refused() {
        let mut spec = specification();
        spec.verdicts[0].items.min = 3;
        assert!(!is_valid(&spec, &LIMITS));
        let mut spec = specification();
        spec.verdicts[0].items.kinds = Box::new([]);
        assert!(!is_valid(&spec, &LIMITS));
        let mut spec = specification();
        spec.verdicts[0].items.kinds[1].kind = spec.verdicts[0].items.kinds[0].kind.clone();
        assert!(!is_valid(&spec, &LIMITS));
        let mut spec = specification();
        spec.report.as_mut().unwrap().fields = Box::new([
            FieldRule { name: b"x".as_slice().into(), max: 1 },
            FieldRule { name: b"x".as_slice().into(), max: 2 },
        ]);
        assert!(!is_valid(&spec, &LIMITS));
    }
}
