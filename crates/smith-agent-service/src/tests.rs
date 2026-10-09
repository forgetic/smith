//! Profile construction stays with the service whose limits it selects.

use alloc::boxed::Box;
use skein_lib::List;

#[test]
fn standard_profile_builds_a_complete_bounded_service() {
    let limits = crate::profile::standard_limits(u64::MAX).expect("standard profile");
    let bound = crate::worst_case(&limits).expect("checked bound");
    assert!(bound > 0);
    let configuration = crate::Config {
        limits,
        domain: smith_domain::Config { endpoints: Box::new([]) },
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
    let service = crate::Service::new(configuration, 1).expect("service");
    assert!(crate::done(&service).is_none());
}
