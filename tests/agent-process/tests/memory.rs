//! Reach receiving maxima while every hosted iteration and drop is metered.
//! The composed bound sums independently bounded layers, so equality with the
//! sum is not expected; the driver attains the declared run context/section
//! and decoded-completion limits rather than only measuring construction.
use skein_lib::{List, Reader, Writer};
use skein_world::domain::heap::Counting;
use smith_agent_process_world::{World, charter, limits};

#[global_allocator]
static HEAP: Counting = Counting;

fn full_context() -> Box<[u8]> {
    let base = charter();
    let mut parts = smith_charter::Charter::decode(&smith_charter::CEILINGS, &mut Reader::new(&base))
        .expect("charter")
        .into_parts();
    assert!(parts.models.is_empty() && parts.conventions.is_none() && parts.tools.host().is_empty());
    assert!(parts.contract.report().as_ref().expect("report").fields().is_empty());
    let count = limits().domain.run.brief_sections;
    let fixed =
        u64::from(count) * (size_of::<smith_domain_run::Section>() as u64 + 1) + parts.main.model().len() as u64;
    parts.instructions =
        vec![b'x'; usize::try_from(limits().domain.run.run_bytes - fixed).expect("payload")].into_boxed_slice();
    parts.brief = List::with_capacity(count);
    for _ in 0..count {
        parts
            .brief
            .push(
                smith_charter::Section::new(
                    &smith_charter::CEILINGS,
                    smith_charter::SectionParts { title: b"x".as_slice().into(), body: Box::new([]) },
                )
                .expect("section"),
            )
            .expect("maximum sections");
    }
    assert_eq!(
        parts.instructions.len() as u64 + fixed,
        limits().domain.run.run_bytes,
        "exact aggregate receiving maximum"
    );
    assert_eq!(parts.brief.len(), count, "maximum owning section array");
    let record = smith_charter::Charter::new(&smith_charter::CEILINGS, parts).expect("full charter");
    let mut writer = Writer::new(usize::try_from(record.measure()).expect("wire size"));
    record.encode(&mut writer).expect("wire");
    writer.finish()
}

#[test]
fn maximum_run_context_and_provider_completion_fit_each_process_at_every_iteration() {
    let mut context = World::new(7, &full_context());
    context.signal();
    context.check_memory();
    assert!(context.settle());
    assert!(context.observed().iter().any(|frame| frame.kind == 0x0106), "full context was admitted, not refused");
    drop(context);
    let mut completion = World::new(7, &charter());
    completion.full_completion();
    completion.check_memory();
    assert!(completion.settle());
    assert!(completion.peer_replied(), "the actual maximum completion was emitted");
    let client = limits().llm.adapter.client;
    assert_eq!(
        completion.provider_answer_shape(),
        (
            usize::try_from(client.dialect.parts).expect("parts"),
            usize::try_from(client.dialect.answer_bytes).expect("payload")
        ),
        "both maximum parts and total provider answer bytes including actual ID/phase metadata reached the host transcript"
    );
    drop(completion);
}
