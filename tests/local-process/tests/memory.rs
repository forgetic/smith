//! The shared shell, actual turn store, TLS peers and finite fixture remain
//! within their checked Host bounds and the separately priced world trace.
use skein_world::domain::heap::{Counting, Meter};
use smith_local_process_world::{Authentication, Files, Placement, World};

#[global_allocator]
static HEAP: Counting = Counting;

#[test]
fn the_composed_world_including_its_shared_shell_fits_its_bound() {
    for placement in [Placement::Spawned, Placement::InProcess] {
        let files = Files::new();
        let meter = Meter::new();
        meter.start();
        let mut world = match placement {
            Placement::Spawned => World::authenticated(10, &files, placement, Authentication::SignIn),
            Placement::InProcess => World::changed(10, &files, placement),
        };
        world.settle();
        let bound = world.worst_case();
        let measured = meter.end();
        assert!(meter.check(measured, bound, &placement) > 0);
        drop(world);
        assert_eq!(meter.held(), 0, "every process and fixture allocation was freed");
    }
}
