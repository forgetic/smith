//! Profile construction stays with the service whose limits it selects.

use alloc::boxed::Box;
use skein_lib::List;

#[test]
fn standard_profile_builds_a_complete_bounded_service() {
    let limits = crate::profile::derive(
        &{
            let mut profile = crate::profile::standard();
            profile.declared.memory = Some(u64::MAX);
            profile
        },
        &crate::profile::Configuration { environment_bytes: 0, endpoints: Box::new([]) },
    )
    .expect("standard profile");
    let configuration = crate::Config {
        limits,
        domain: smith_domain::Config { endpoints: Box::new([]), models: Box::new([]) },
        channel_endpoints: smith_protocol_channel::Endpoints::new(List::with_capacity(0)),
        llm_endpoints: smith_protocol_llm::Endpoints::new(
            Box::new([]),
            crate::profile::ENDPOINTS,
            crate::profile::ACCOUNTS,
        )
        .expect("empty endpoints"),
        environment: Box::new([]),
        stream_mode: skein_channel::StreamMode::Two,
        capture_prompts: false,
    };
    let bound = crate::worst_case(&configuration.limits, &configuration.llm_endpoints).expect("checked bound");
    assert!(bound > 0);
    let service = crate::Service::new(configuration, 1).expect("service");
    assert!(crate::done(&service).is_none());
}

#[test]
fn random_tool_render_caps_derive_a_machine_that_holds_them() {
    let mut rng = skein_lib::Rng::new(941);
    for _ in 0_u32..256 {
        let mut profile = crate::profile::standard();
        profile.declared.memory = Some(u64::MAX);
        profile.declared.tool_payload = u32::try_from(rng.next_u64() % 65536 + 1).expect("bounded payload");
        profile.declared.read_window =
            u32::try_from(rng.next_u64() % u64::from(profile.declared.tool_payload) + 1).expect("bounded read");
        profile.declared.shell_head = u32::try_from(rng.next_u64() % 4096 + 1).expect("bounded head");
        profile.declared.shell_tail = u32::try_from(rng.next_u64() % 4096 + 1).expect("bounded tail");
        profile.declared.search_hits = u32::try_from(rng.next_u64() % 32 + 1).expect("bounded hits");
        profile.declared.search_bytes = u32::try_from(rng.next_u64() % 4096 + 1).expect("bounded search");
        profile.declared.list_entries = u32::try_from(rng.next_u64() % 64 + 1).expect("bounded listing");
        let configuration = crate::profile::Configuration { environment_bytes: 37, endpoints: Box::new([]) };
        let limits = crate::profile::derive(&profile, &configuration).expect("bounded declarations derive");
        let tools = limits.domain.session.tools;
        let machine = limits.machine;
        assert!(machine.output_bytes >= tools.shell_head);
        assert!(machine.output_bytes >= tools.shell_tail);
        assert!(machine.file_bytes >= tools.file_bytes);
        assert!(machine.entries >= tools.list_entries);
        assert!(machine.entry_bytes >= tools.list_bytes);
        assert!(machine.search_hits >= tools.search_hits);
        assert!(machine.search_bytes >= tools.search_bytes);
        assert!(machine.path_bytes >= tools.path_bytes);
        assert_eq!(machine.env_bytes, configuration.environment_bytes);
    }
    let mut profile = crate::profile::standard();
    profile.declared.memory = Some(u64::MAX);
    profile.declared.search_bytes = u32::MAX;
    assert_eq!(
        crate::profile::derive(
            &profile,
            &crate::profile::Configuration { environment_bytes: 0, endpoints: Box::new([]) }
        )
        .err(),
        Some(crate::profile::ProfileError::Receiving)
    );
}
