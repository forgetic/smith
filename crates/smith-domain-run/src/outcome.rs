//! What counts as done (domain/run.md, section 14): the outcome spec a
//! charter carries, the outcome an LLM declares when it finishes, and
//! [`judge`], which checks the one against the other. Names are labels,
//! compared byte for byte and never interpreted.

use alloc::boxed::Box;
use core::mem::size_of;

use skein_lib::List;
use skein_lib::bytes::copy_of;

use crate::charter::{count, len};
use crate::limits::Limits;

/// What a run may finish with: a change, a verdict from a closed list, or
/// either.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct OutcomeSpec {
    /// Whether it may finish with a change, the diff of its checkout with a
    /// title and body for its pull request, and on what terms.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub change: Option<ChangeSpec>,
    /// Closed verdict rules supplied by the host. An empty slice permits no verdict;
    /// admission bounds the rule count by `Limits.verdicts` and the aggregate
    /// charter ownership by `Limits.run_bytes`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub verdicts: Box<[VerdictRule]>,
}

/// The terms on which a run may finish with a change.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChangeSpec {
    /// Whether the checks of the repositories that have them must pass before
    /// the change is pushed (domain/run.md, section 14).
    pub checks: bool,
}

/// A verdict a run may finish with, and its contract.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct VerdictRule {
    /// Boundary name, compared byte for byte; it carries no authority by itself.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub name: Box<[u8]>,
    /// How many children a verdict of this name has.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub children: Children,
    /// The kinds its children may be.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub kinds: Box<[Box<[u8]>]>,
    /// The fields each of its children must carry.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub fields: Box<[Box<[u8]>]>,
}

/// At least `min`, at most `max`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Children {
    /// Inclusive minimum count required by this contract.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub min: u32,
    /// Inclusive maximum count allowed by this contract.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub max: u32,
}

/// An outcome an LLM declares when it finishes, typed by the protocol layer
/// from the input it wrote.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum Declared {
    /// Declared copy-baseline checkout change with title and body.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Change(
        /// Protocol-decoded change bytes; the run checks aggregate `Limits.outcome_bytes`
        /// before shape judgement, then required checks and host delivery before acceptance.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        Change,
    ),
    /// Declared copy-baseline closed-list verdict and its contract items.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Verdict(
        /// Protocol-decoded verdict and item bytes; the run checks aggregate
        /// `Limits.outcome_bytes` before validating the host's closed verdict contract.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        Verdict,
    ),
}

/// Protocol-decoded LLM declaration attached to the actual checkout change.
/// Title and body must be nonempty. Before shape judgement, run finishing
/// bounds their aggregate owned payload by `Limits.outcome_bytes`; successful
/// host delivery, with required checks, precedes the accepted terminal.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Change {
    /// LLM-supplied nonempty delivery title; its owned bytes contribute to the aggregate outcome cap.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub title: Box<[u8]>,
    /// LLM-supplied nonempty delivery body; its owned bytes contribute to the aggregate outcome cap.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub body: Box<[u8]>,
}

/// Protocol-decoded LLM verdict, checked against the host's closed list and
/// item contract. The run bounds all boxes and payload bytes together by
/// `Limits.outcome_bytes` before shape judgement. An accepted verdict ends the run.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Verdict {
    /// Boundary name, compared byte for byte; it carries no authority by itself.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub name: Box<[u8]>,
    /// What the LLM says about it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub body: Box<[u8]>,
    /// Declared result items, checked against the charter's count, kind and field rules.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub children: Box<[Child]>,
}

/// One of a verdict's children: a comment, an issue to open, whatever its
/// kind names.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Child {
    /// Typed entry classification or byte label required by the enclosing contract.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub kind: Box<[u8]>,
    /// Named values of this result item; required names must occur exactly once.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub fields: Box<[Field]>,
}

/// LLM-supplied named value in a verdict item. Repeated names are invalid;
/// required names must exist with nonempty values. Extra fields are permitted
/// and may be empty. Names and values contribute to the run's checked aggregate
/// `Limits.outcome_bytes` bound, including the boxed `Field` storage.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Field {
    /// Boundary name, compared byte for byte; it carries no authority by itself.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub name: Box<[u8]>,
    /// LLM-supplied result-field bytes; nonempty only when this name is required by the rule.
    /// Counted with the name and box against the aggregate outcome ownership cap.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub value: Box<[u8]>,
}

/// Something wrong with a declared outcome, for the LLM to fix. Children are
/// counted from zero, in the order the LLM gave them.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Problem {
    /// The outcome holds more than `max` bytes, as a run counts them.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    TooLarge {
        /// Inclusive maximum count allowed by this contract.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        max: u64,
    },
    /// The run may not finish with a change.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    ChangeNotAllowed,
    /// The run may not finish with a verdict.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    VerdictNotAllowed,
    /// No verdict the run may finish with has this name.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    UnknownVerdict,
    /// The verdict has fewer children than its contract requires.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    TooFewChildren {
        /// Inclusive minimum count required by this contract.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        min: u32,
    },
    /// The verdict has more children than its contract allows.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    TooManyChildren {
        /// Inclusive maximum count allowed by this contract.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        max: u32,
    },
    /// A child is of a kind its verdict does not allow.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    KindNotAllowed {
        /// Zero-based result-item index whose contract failed.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        child: u32,
    },
    /// A change with no title for its pull request.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    EmptyTitle,
    /// A change with no body for its pull request.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    EmptyBody,
    /// A child lacks `field`, which its verdict requires of every child.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    MissingField {
        /// Zero-based result-item index whose contract failed.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        child: u32,
        /// Name of the required input member which failed validation.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        field: Box<[u8]>,
    },
    /// A child has `field`, which its verdict requires, with nothing in it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    EmptyField {
        /// Zero-based result-item index whose contract failed.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        child: u32,
        /// Name of the required input member which failed validation.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        field: Box<[u8]>,
    },
    /// A child has `field` more than once.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    RepeatedField {
        /// Zero-based result-item index whose contract failed.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        child: u32,
        /// Name of the required input member which failed validation.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        field: Box<[u8]>,
    },
}

/// What is wrong with a declared outcome: the first problems found, at most
/// [`Problems::LISTED`] of them in the order they were found, and how many
/// more there were.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Problems {
    /// First bounded outcome-validation problems, in validation order.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub listed: Box<[Problem]>,
    /// Problems or matches omitted after the retained bound.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub more: u32,
}

impl Problems {
    /// Enough for the LLM to see what is wrong, few enough to keep what it is
    /// told small.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub const LISTED: u32 = 8;
}

/// Pure shape judgement of protocol-decoded `declared` against the host's
/// `spec`: returns success or the first eight problems and a count of omissions.
/// This function checks no byte cap. Run finishing first computes checked
/// boxed-storage-plus-payload ownership and refuses values beyond
/// `Limits.outcome_bytes`; callers invoking this helper alone supply their own
/// ownership bounds.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub fn judge(spec: &OutcomeSpec, declared: &Declared) -> Result<(), Problems> {
    let mut found = Found { listed: List::with_capacity(Problems::LISTED), more: 0 };
    match declared {
        Declared::Change(Change { title, body }) => {
            if spec.change.is_none() {
                found.add(Problem::ChangeNotAllowed);
            }
            if title.is_empty() {
                found.add(Problem::EmptyTitle);
            }
            if body.is_empty() {
                found.add(Problem::EmptyBody);
            }
        }
        Declared::Verdict(verdict) => judge_verdict(&spec.verdicts, verdict, &mut found),
    }
    if found.listed.is_empty() {
        return Ok(());
    }
    Err(Problems { listed: found.listed.into_boxed(), more: found.more })
}

/// The problems found so far.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
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

    /// Adds a problem about the child `child`'s field `field`, copying the
    /// field's name only if the problem is listed.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    fn add_field(&mut self, child: u32, field: &[u8], problem: FieldProblem) {
        if self.listed.room() == 0 {
            self.more = self.more.saturating_add(1);
            return;
        }
        let field = copy_of(field);
        self.add(match problem {
            FieldProblem::Missing => Problem::MissingField { child, field },
            FieldProblem::Empty => Problem::EmptyField { child, field },
            FieldProblem::Repeated => Problem::RepeatedField { child, field },
        });
    }
}

/// What is wrong with one of a child's fields.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy)]
enum FieldProblem {
    Missing,
    Empty,
    Repeated,
}

fn judge_verdict(rules: &[VerdictRule], verdict: &Verdict, found: &mut Found) {
    if rules.is_empty() {
        found.add(Problem::VerdictNotAllowed);
        return;
    }
    let named = match rule_named(rules, &verdict.name) {
        Some(index) => rules.get(index),
        None => None,
    };
    let Some(rule) = named else {
        found.add(Problem::UnknownVerdict);
        return;
    };
    let Children { min, max } = rule.children;
    let children = count(verdict.children.len());
    if children < min {
        found.add(Problem::TooFewChildren { min });
    }
    if children > max {
        found.add(Problem::TooManyChildren { max });
    }
    let mut index: u32 = 0;
    for child in &verdict.children {
        if !names(&rule.kinds, &child.kind) {
            found.add(Problem::KindNotAllowed { child: index });
        }
        for field in &rule.fields {
            let given = match field_named(&child.fields, field) {
                Some(at) => child.fields.get(at),
                None => None,
            };
            match given {
                None => found.add_field(index, field, FieldProblem::Missing),
                Some(given) if given.value.is_empty() => found.add_field(index, field, FieldProblem::Empty),
                Some(_) => {}
            }
        }
        // A name given more than once is said once, where it is first given.
        for (at, field) in child.fields.iter().enumerate() {
            let earlier = child.fields.get(..at).unwrap_or_default();
            let later = child.fields.get(at.saturating_add(1)..).unwrap_or_default();
            if field_named(earlier, &field.name).is_none() && field_named(later, &field.name).is_some() {
                found.add_field(index, &field.name, FieldProblem::Repeated);
            }
        }
        index = index.saturating_add(1);
    }
}

/// Where among `rules` the verdict named `name` is.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn rule_named(rules: &[VerdictRule], name: &[u8]) -> Option<usize> {
    for (index, rule) in rules.iter().enumerate() {
        if *rule.name == *name {
            return Some(index);
        }
    }
    None
}

/// Whether `labels` has `label` among them.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn names(labels: &[Box<[u8]>], label: &[u8]) -> bool {
    for candidate in labels {
        if **candidate == *label {
            return true;
        }
    }
    false
}

/// Where among `fields` the first named `name` is.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn field_named(fields: &[Field], name: &[u8]) -> Option<usize> {
    for (at, field) in fields.iter().enumerate() {
        if *field.name == *name {
            return Some(at);
        }
    }
    None
}

/// Whether `spec` fits `limits` and can be met: it allows some outcome, lists
/// no more verdicts than a run may hold and no name twice, and each verdict's
/// contract can be met, requiring no more children than it allows and giving
/// a kind for children to be if it allows any.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn is_valid(spec: &OutcomeSpec, limits: &Limits) -> bool {
    let OutcomeSpec { change, verdicts } = spec;
    if change.is_none() && verdicts.is_empty() {
        return false;
    }
    if count(verdicts.len()) > limits.verdicts {
        return false;
    }
    for (index, rule) in verdicts.iter().enumerate() {
        let Children { min, max } = rule.children;
        if min > max || (max > 0 && rule.kinds.is_empty()) {
            return false;
        }
        for other in verdicts.get(index.saturating_add(1)..).unwrap_or_default() {
            if other.name == rule.name {
                return false;
            }
        }
    }
    true
}

/// The bytes `spec` holds beyond its fixed size, counted as the charter's are.
/// `None` past a `u64`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn cost(spec: &OutcomeSpec) -> Option<u64> {
    let rule = u64::try_from(size_of::<VerdictRule>()).ok()?;
    let mut cost: u64 = 0;
    for VerdictRule { name, children: _, kinds, fields } in &spec.verdicts {
        cost = cost
            .checked_add(rule)?
            .checked_add(len(name)?)?
            .checked_add(labels(kinds)?)?
            .checked_add(labels(fields)?)?;
    }
    Some(cost)
}

/// The bytes `declared` holds beyond its fixed size: each part held in a box
/// at its fixed size, plus its payload. `None` past a `u64`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn declared_cost(declared: &Declared) -> Option<u64> {
    match declared {
        Declared::Change(Change { title, body }) => len(title)?.checked_add(len(body)?),
        Declared::Verdict(Verdict { name, body, children }) => {
            let child = u64::try_from(size_of::<Child>()).ok()?;
            let field = u64::try_from(size_of::<Field>()).ok()?;
            let mut cost = len(name)?.checked_add(len(body)?)?;
            for Child { kind, fields } in children {
                cost = cost.checked_add(child)?.checked_add(len(kind)?)?;
                for Field { name, value } in fields {
                    cost = cost.checked_add(field)?.checked_add(len(name)?)?.checked_add(len(value)?)?;
                }
            }
            Some(cost)
        }
    }
}

/// The problems of an outcome that holds more than `max` bytes.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn too_large(max: u64) -> Problems {
    Problems { listed: Box::new([Problem::TooLarge { max }]), more: 0 }
}

fn labels(labels: &[Box<[u8]>]) -> Option<u64> {
    let label = u64::try_from(size_of::<Box<[u8]>>()).ok()?;
    let mut cost: u64 = 0;
    for name in labels {
        cost = cost.checked_add(label)?.checked_add(len(name)?)?;
    }
    Some(cost)
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;

    use skein_lib::bytes::copy_of;

    use super::{
        Change, ChangeSpec, Child, Children, Declared, Field, OutcomeSpec, Problem, Problems, Verdict, VerdictRule,
        judge,
    };

    fn labels(names: &[&[u8]]) -> Box<[Box<[u8]>]> {
        let mut labels = skein_lib::List::with_capacity(4);
        for name in names {
            labels.push(copy_of(name)).expect("a few labels");
        }
        labels.into_boxed()
    }

    /// A review: approve with no children, or request changes with one to
    /// three comments, each blocking or a nit, with a path and a body.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    fn review(change: bool) -> OutcomeSpec {
        let approve = VerdictRule {
            name: copy_of(b"approve"),
            children: Children { min: 0, max: 0 },
            kinds: labels(&[]),
            fields: labels(&[]),
        };
        let request = VerdictRule {
            name: copy_of(b"request-changes"),
            children: Children { min: 1, max: 3 },
            kinds: labels(&[b"blocking", b"nit"]),
            fields: labels(&[b"path", b"body"]),
        };
        OutcomeSpec { change: change.then_some(ChangeSpec { checks: false }), verdicts: Box::new([approve, request]) }
    }

    fn change() -> Declared {
        Declared::Change(Change { title: copy_of(b"Fix the parser"), body: copy_of(b"It now accepts tabs.") })
    }

    fn verdict(name: &[u8], children: Box<[Child]>) -> Declared {
        Declared::Verdict(Verdict { name: copy_of(name), body: copy_of(b"See the comments."), children })
    }

    fn child(kind: &[u8], fields: &[&[u8]]) -> Child {
        let mut list = skein_lib::List::with_capacity(4);
        for name in fields {
            list.push(Field { name: copy_of(name), value: copy_of(b"...") }).expect("a few fields");
        }
        Child { kind: copy_of(kind), fields: list.into_boxed() }
    }

    fn comment() -> Child {
        child(b"nit", &[b"path", b"body"])
    }

    /// A comment whose body is empty.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    fn empty() -> Child {
        let fields = Box::new([
            Field { name: copy_of(b"path"), value: copy_of(b"a.rs") },
            Field { name: copy_of(b"body"), value: copy_of(b"") },
        ]);
        Child { kind: copy_of(b"nit"), fields }
    }

    /// A comment that gives its path twice, and its body once.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    fn twice() -> Child {
        let fields = Box::new([
            Field { name: copy_of(b"path"), value: copy_of(b"a.rs") },
            Field { name: copy_of(b"body"), value: copy_of(b"Nit.") },
            Field { name: copy_of(b"path"), value: copy_of(b"b.rs") },
            Field { name: copy_of(b"path"), value: copy_of(b"c.rs") },
        ]);
        Child { kind: copy_of(b"nit"), fields }
    }

    fn problems(listed: Box<[Problem]>, more: u32) -> Result<(), Problems> {
        Err(Problems { listed, more })
    }

    fn missing(child: u32, field: &[u8]) -> Problem {
        Problem::MissingField { child, field: copy_of(field) }
    }

    #[test]
    fn outcomes_are_judged_against_the_spec() {
        let cases: [(OutcomeSpec, Declared, Result<(), Problems>); 15] = [
            // What the spec allows.
            (review(true), change(), Ok(())),
            (OutcomeSpec { change: Some(ChangeSpec { checks: false }), verdicts: Box::new([]) }, change(), Ok(())),
            (review(false), verdict(b"approve", Box::new([])), Ok(())),
            (
                review(false),
                verdict(b"request-changes", Box::new([comment(), child(b"blocking", &[b"body", b"path", b"line"])])),
                Ok(()),
            ),
            // What it does not.
            (review(false), change(), problems(Box::new([Problem::ChangeNotAllowed]), 0)),
            (
                OutcomeSpec { change: Some(ChangeSpec { checks: false }), verdicts: Box::new([]) },
                verdict(b"approve", Box::new([])),
                problems(Box::new([Problem::VerdictNotAllowed]), 0),
            ),
            (review(false), verdict(b"reject", Box::new([])), problems(Box::new([Problem::UnknownVerdict]), 0)),
            (
                review(false),
                verdict(b"request-changes", Box::new([])),
                problems(Box::new([Problem::TooFewChildren { min: 1 }]), 0),
            ),
            (
                review(false),
                verdict(b"request-changes", Box::new([comment(), comment(), comment(), comment()])),
                problems(Box::new([Problem::TooManyChildren { max: 3 }]), 0),
            ),
            (
                review(false),
                verdict(b"approve", Box::new([comment()])),
                problems(Box::new([Problem::TooManyChildren { max: 0 }, Problem::KindNotAllowed { child: 0 }]), 0),
            ),
            (
                review(false),
                verdict(b"request-changes", Box::new([comment(), child(b"praise", &[b"path", b"body"])])),
                problems(Box::new([Problem::KindNotAllowed { child: 1 }]), 0),
            ),
            (
                review(false),
                verdict(b"request-changes", Box::new([child(b"nit", &[b"body"]), child(b"blocking", &[])])),
                problems(Box::new([missing(0, b"path"), missing(1, b"path"), missing(1, b"body")]), 0),
            ),
            // A change needs a title and a body; a required field, something
            // in it; and no field may be given twice.
            (
                OutcomeSpec { change: Some(ChangeSpec { checks: false }), verdicts: Box::new([]) },
                Declared::Change(Change { title: copy_of(b""), body: copy_of(b"") }),
                problems(Box::new([Problem::EmptyTitle, Problem::EmptyBody]), 0),
            ),
            (
                review(false),
                verdict(b"request-changes", Box::new([empty(), twice()])),
                problems(
                    Box::new([
                        Problem::EmptyField { child: 0, field: copy_of(b"body") },
                        Problem::RepeatedField { child: 1, field: copy_of(b"path") },
                    ]),
                    0,
                ),
            ),
            // Labels are compared byte for byte.
            (
                review(false),
                verdict(b"request-changes", Box::new([child(b"Nit", &[b"path", b"body "])])),
                problems(Box::new([Problem::KindNotAllowed { child: 0 }, missing(0, b"body")]), 0),
            ),
        ];
        for (index, (spec, declared, judged)) in cases.into_iter().enumerate() {
            assert_eq!(judge(&spec, &declared), judged, "case {index}");
        }
    }

    #[test]
    fn the_problems_listed_are_bounded_and_the_rest_counted() {
        let mut children = skein_lib::List::with_capacity(5);
        for _ in 0_u32..5 {
            children.push(child(b"praise", &[])).expect("room for five");
        }
        let Err(problems) = judge(&review(false), &verdict(b"request-changes", children.into_boxed())) else {
            panic!("five children of no kind, with no fields, against a contract of three");
        };
        let listed = [
            Problem::TooManyChildren { max: 3 },
            Problem::KindNotAllowed { child: 0 },
            missing(0, b"path"),
            missing(0, b"body"),
            Problem::KindNotAllowed { child: 1 },
            missing(1, b"path"),
            missing(1, b"body"),
            Problem::KindNotAllowed { child: 2 },
        ];
        assert_eq!(&*problems.listed, &listed);
        assert_eq!(u32::try_from(problems.listed.len()), Ok(Problems::LISTED));
        // Three problems for each of the last two children, and one more for
        // the third.
        assert_eq!(problems.more, 2 + 3 + 3);
    }
}
