# Agent guidance for smith

- Before merging to main, run every check in
  `docs/development/workflow.md` on the final rebased tip. Main moves only
  after they pass; a Markdown-only change skips the checks as that
  document specifies. Rust doc comments do not skip them.
- The focused suite takes at most 15 seconds and the fuzzy suite at most
  60 seconds. Measure worlds on an idle machine with the workflow's
  serial profile; keep additions within those budgets.
- smith follows skein's `docs/foundation/programming-model.md`,
  `testing-strategy.md` and `notes.md`. Read them before writing code.
  Source citations use those file names and their sections.
- smith's contracts are in `docs/design/domain/`. Code cites them as
  `domain/<file>.md` with the applicable sections. Each public type,
  variant, field and entry function documents its sender, contract,
  terminal outcome and bounds where applicable. Module docs say what
  state is kept, what is never known, and the entry points and contracts.
- Production domains use the strict Rust subset: exhaustive own-enum
  matches, bounded skein containers and checked arithmetic. Review the
  rules the compiler cannot check, including no own-enum if-let or
  let-else, tuple scrutinees, closures, `loop`, `while`, application traits
  or generics. Test harnesses are ordinary Rust with deterministic clocks
  and seeds, following programming-model.md, section 10.2.
- Re-export public names explicitly, use descriptive parameter names,
  separate items with blank lines, and keep the standard `Result` name
  for fallible operations.
- Reuse skein's generic test kit. Do not make smith a second owner of
  generic schedules, replay, referee plumbing or counting allocators.
  A missing shared mechanism belongs in skein before its smith consumer.
- Work locally in isolated branches and merge with `--ff-only`. Do not
  publish or push unless the session explicitly authorizes it.
