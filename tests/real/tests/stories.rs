use skein_world::domain::heap::Counting;

#[global_allocator]
static HEAP: Counting = Counting;

use skein_io::kernel;
use smith_local_process_world::{
    Placement,
    referee::{CheckoutRead, Ending, review},
};
use smith_real_world::{Scratch, World};

#[test]
fn a_first_run_signs_in_and_keeps_its_token() {
    let scratch = Scratch::new();
    let mut world = World::authenticated(11, &scratch, Placement::Spawned);
    world.settle();
    let seen = world.seen();
    let oauth = seen.oauth.expect("issuer observations");
    assert_eq!(oauth.posts, 1);
    assert_eq!(oauth.pages, 1);
    assert!(oauth.browser_replied && oauth.saved_before_query);
    let tokens = smith_local_shell::local_tokens::Tokens::new(
        &world.scenario.launch.token_directory,
        smith_local_shell::local_host::token_limits(),
    )
    .expect("private token store");
    let saved = tokens.load(0).expect("token load").expect("saved grant");
    assert_eq!(saved.access_token.as_ref(), b"access-new");
    assert_eq!(saved.refresh_token.as_deref(), Some(b"refresh-new".as_slice()));
    assert_eq!(seen.exit, Some(kernel::Exit::Code(0)));
}

#[test]
fn a_chat_ends_with_a_commit_in_place() {
    let scratch = Scratch::new();
    let mut world = World::changed(12, &scratch, Placement::Spawned);
    let ending = world.scenario.ending.clone();
    world.settle();
    review(&world.seen(), world.checkout(), ending).assert_passed(12);
    let head = world.checkout().head().expect("committed checkout");
    let files = world.checkout().files(&head);
    assert_eq!(files[b"original.txt".as_slice()], b"edited\n");
    assert_eq!(files[b"result.txt".as_slice()], b"new\n");
    assert_eq!(files[b"command.txt".as_slice()], b"ran\n");
    assert_eq!(files[b"checks-ran.txt".as_slice()], b"checked\n");
    let seen = world.seen();
    assert_eq!(seen.queries.len(), 7, "every scripted tool reached the real agent");
    for (query, name) in seen.queries.iter().skip(2).zip([b"read".as_slice(), b"search", b"edit", b"write", b"shell"]) {
        let id = query
            .messages
            .iter()
            .flat_map(|message| message.parts.iter())
            .find_map(|part| match part {
                skein_fake_llm_domain::api::Part::ToolCall { id, name: called, .. } if called.as_ref() == name => {
                    Some(id)
                }
                _ => None,
            })
            .expect("next provider request includes the tool call");
        let output = query
            .messages
            .iter()
            .flat_map(|message| message.parts.iter())
            .find_map(|part| match part {
                skein_fake_llm_domain::api::Part::ToolOutput { id: answered, output, .. } if answered == id => {
                    Some(output)
                }
                _ => None,
            })
            .expect("actual tool terminal reaches the provider");
        assert!(
            !output.starts_with(b"Error:"),
            "tool {}: {}",
            String::from_utf8_lossy(name),
            String::from_utf8_lossy(output)
        );
        if name == b"search" {
            assert!(
                output.windows(12).any(|bytes| bytes == b"original.txt"),
                "actual rg output names the matched file"
            );
            assert!(output.windows(8).any(|bytes| bytes == b"original"), "actual rg output contains the matched text");
        }
        if name == b"read" {
            assert!(output.windows(8).any(|bytes| bytes == b"original"), "actual file bytes reached the provider");
        }
        if name == b"shell" {
            assert!(output.starts_with(b"exit code 0"), "the real command exited successfully");
        }
    }
}

#[test]
fn a_second_run_resumes_the_chat_from_its_files() {
    let scratch = Scratch::new();
    let mut first = World::new(13, &scratch, Placement::Spawned, b"First answer");
    first.settle();
    assert_eq!(first.turns(), 2);
    let messages = first.seen().queries[0].messages.len();
    drop(first);
    let mut second = World::new(14, &scratch, Placement::Spawned, b"Second answer");
    second.settle();
    assert_eq!(second.turns(), 4);
    let seen = second.seen();
    assert!(seen.queries[0].messages.len() > messages);
    assert!(
        seen.queries[0].messages.iter().flat_map(|message| message.parts.iter()).any(|part| {
            matches!(part, skein_fake_llm_domain::api::Part::ToolCall { name, arguments, .. }
            if name.as_ref() == b"finish" && arguments.windows(12).any(|bytes| bytes == b"First answer"))
        }),
        "actual stored history reached the provider"
    );
    assert_eq!(seen.exit, Some(kernel::Exit::Code(0)));
}

#[test]
fn the_same_chat_runs_with_its_agent_in_process() {
    let first = Scratch::new();
    let second = Scratch::new();
    let mut hosted = World::new(15, &first, Placement::Spawned, b"First answer");
    let mut colocated = World::new(15, &second, Placement::InProcess, b"First answer");
    hosted.settle();
    colocated.settle();
    let spawned = hosted.seen();
    let in_process = colocated.seen();
    assert_eq!(spawned.shown, in_process.shown);
    assert_eq!(spawned.queries, in_process.queries);
    assert_eq!(spawned.exit, in_process.exit);
    assert_eq!(hosted.turns(), colocated.turns());
    review(&in_process, colocated.checkout(), Ending::Report(b"First answer".to_vec())).assert_passed(15);
}
