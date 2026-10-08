//! Opt-in shipped-binary stories against real backends (testing.md, section 2.3).
//! The `live` nextest profile selects this binary; the merge gate excludes it.

#[path = "support/live.rs"]
#[allow(dead_code)]
mod live;
#[allow(dead_code)]
mod support;

#[test]
fn bootstrap_settings() {
    if !live::enabled() {
        return;
    }
    let Some(directory) = live::setting("SMITH_TEST_LIVE_BOOTSTRAP_DIR") else {
        return;
    };
    let directory = live::safe_directory(std::path::Path::new(&directory));
    assert!(directory.is_absolute(), "SMITH_TEST_LIVE_BOOTSTRAP_DIR must be absolute");
    let environment = live::Environment::load(true);
    std::fs::create_dir_all(&directory).expect("bootstrap settings directory");
    for backend in environment.backends {
        let state = directory.join(&backend.name);
        std::fs::create_dir_all(&state).expect("bootstrap state directory");
        std::fs::write(
            state.join("settings.json"),
            serde_json::to_vec_pretty(&backend.settings(&state, false)).expect("bootstrap JSON"),
        )
        .expect("bootstrap settings");
    }
}
