//! Probe: which capabilities does a LabRuntime task expose to our shell
//! pattern? The drive loop needs the ambient `Cx` (for the clock); spawning is
//! a separate capability.
//!
//! This is exploratory — it asserts what works so the simulation harness builds
//! on solid ground rather than a guess.
//!
//! Note: skein exposes no ambient "current runtime handle" accessor at all (it
//! was removed). Production code threads a `RuntimeHandle` in explicitly (the
//! bin builds the engine runtime via `build_runtime` and passes `handle()` into
//! `run_worker`), and the shells take a `Spawner` seam so the lab can substitute
//! its own — so there is nothing handle-shaped to probe here; only the ambient
//! `Cx` is.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use skein::lab::{LabConfig, LabRuntime};
use skein::types::Budget;

#[test]
fn ambient_cx_resolves_inside_a_lab_task() {
    let mut runtime = LabRuntime::new(LabConfig::new(1).with_auto_advance().max_steps(100_000));
    let region = runtime.state.create_root_region(Budget::INFINITE);

    let cx_resolved = Arc::new(AtomicU32::new(0));
    let cx_resolved_in = Arc::clone(&cx_resolved);

    let (task_id, _h) = runtime
        .state
        .create_task(region, Budget::INFINITE, async move {
            // The drive loop only needs the ambient Cx (for the clock).
            if skein::cx::Cx::current().is_some() {
                cx_resolved_in.fetch_add(1, Ordering::SeqCst);
            }
        })
        .expect("create lab task");
    runtime.scheduler.lock().schedule(task_id, 0);
    runtime.run_with_auto_advance();

    // Document the lab's capabilities for the harness design: the Cx (clock) is
    // available inside a lab task. Spawning is NOT ambient — the shells take a
    // Spawner seam the lab substitutes — which is why there is no runtime handle
    // to probe for here.
    assert_eq!(
        cx_resolved.load(Ordering::SeqCst),
        1,
        "the ambient Cx (drive-loop clock) should resolve inside a lab task"
    );
}
