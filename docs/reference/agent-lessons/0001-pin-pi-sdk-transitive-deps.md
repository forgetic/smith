# Lesson 0001: Pin `pi_agent_rust`'s transitive deps when the SDK won't compile

## Tags

`tooling`, `rust`, `agents`, `dependencies`, `pi-sdk`

## Trigger

Adding `pi_agent_rust = "0.1.13"` first failed not in Smith code, but inside the
transitive dependency `asupersync 0.3.1`, with errors like `no field
'fallback_active' on type 'Result<DecisionOutcome, …>'`.

## What went wrong

`pi_agent_rust 0.1.13` pins `asupersync =0.3.1`, which declares
`franken-decision = "0.3.1"` as a caret range. A fresh downstream resolve picked
`franken-decision 0.3.2`, whose API had changed, while `asupersync 0.3.1` still
used the old return shape. The SDK's own lockfile had kept the compatible
`franken-decision 0.3.1`.

## Steering for future agents

When a fresh crate fails inside a transitive dependency, suspect version skew
before touching vendored source. Run `cargo tree -i <crate>` and compare against
the upstream crate's own `Cargo.lock`. Fix by pinning the drifted dependency in
Smith's `Cargo.lock`:

```sh
cargo update -p franken-decision --precise 0.3.1
```

Re-check the workaround before bumping `pi_agent_rust`.

## Where this is now documented

- `README.md` dependency note.
- `Cargo.toml` workspace comment.
- Smith `Cargo.lock`.
