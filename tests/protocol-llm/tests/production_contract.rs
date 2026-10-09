//! Charter-driven schema generation and typed finish/deliver inputs.

use skein_fake_llm_domain::api::{Finish, Line, Script, Turn};
use skein_lib::Token;
use smith_domain::{llm, run};
use smith_protocol_llm_world::adapter::{self as adapter, Limits};
use smith_protocol_llm_world::wire;

fn limits() -> Limits {
    Limits {
        client: skein_llm_world::limits(),
        tool_bytes: 32768,
        rendered_result: skein_llm_world::limits().dialect.string_bytes,
        shell_default: skein_lib::Duration::from_secs(120),
        shell_maximum: skein_lib::Duration::from_secs(1200),
    }
}

fn field(name: &[u8], max: u32) -> run::outcome::FieldRule {
    run::outcome::FieldRule { name: name.into(), max }
}

fn contract() -> run::outcome::OutcomeSpec {
    run::outcome::OutcomeSpec {
        change: Some(run::outcome::ChangeSpec { checks_must_pass: true, fields: Box::new([field(b"ticket", 12)]) }),
        verdicts: Box::new([run::outcome::VerdictRule {
            name: b"approve".as_slice().into(),
            text_max: 40,
            fields: Box::new([field(b"source", 20)]),
            items: run::outcome::ItemSpec {
                min: 1,
                max: 2,
                kinds: Box::new([run::outcome::ItemRule {
                    kind: b"observation".as_slice().into(),
                    fields: Box::new([field(b"path", 32)]),
                }]),
            },
        }]),
        report: Some(run::outcome::TextSpec { max: 80, fields: Box::new([field(b"source", 20)]) }),
        failure: Some(run::outcome::TextSpec { max: 60, fields: Box::new([field(b"cause", 24)]) }),
    }
}

fn served(ask: llm::Decoded) -> run::Ask {
    match ask {
        llm::Decoded::Served { ask } => ask,
        llm::Decoded::Owned { .. } | llm::Decoded::Invalid { .. } => panic!("typed run ask"),
    }
}

#[test]
fn every_contract_form_yields_a_bounded_json_schema() {
    let all = contract();
    let forms = [
        run::outcome::OutcomeSpec { change: all.change.clone(), verdicts: Box::new([]), report: None, failure: None },
        run::outcome::OutcomeSpec { change: None, verdicts: all.verdicts.clone(), report: None, failure: None },
        run::outcome::OutcomeSpec { change: None, verdicts: Box::new([]), report: all.report.clone(), failure: None },
        run::outcome::OutcomeSpec { change: None, verdicts: Box::new([]), report: None, failure: all.failure.clone() },
    ];
    for specification in &forms {
        let schema = adapter::finish_schema(specification, limits().client.dialect.document_bytes)
            .expect("contract-derived schema fits the dialect");
        let document = skein_llm::Json::from_bytes(&schema, &limits().client.dialect)
            .expect("scripted peer accepts a complete JSON object schema");
        assert_eq!(document.as_tokens().first(), Some(&skein_json::Token::ObjectStart));
        assert!(schema.windows(b"\"maxLength\"".len()).any(|part| part == b"\"maxLength\""));
    }
    let all_schema = adapter::finish_schema(&all, limits().client.dialect.document_bytes).expect("four forms");
    for literal in [b"approve".as_slice(), b"observation", b"maxItems", b"minItems", b"source"] {
        assert!(all_schema.windows(literal.len()).any(|part| part == literal));
    }
    let delivery =
        adapter::deliver_schema(all.change.as_ref().expect("separate grant"), 4096).expect("mid-run change fields");
    assert!(delivery.windows(b"ticket".len()).any(|part| part == b"ticket"));
}

#[test]
fn a_bad_finish_is_corrected_and_each_declared_form_is_typed() {
    let limits = limits();
    let contract = contract();
    let invalid = adapter::decode_finish(br#"{"form":"report","fields":{"source":"test"}}"#, &contract, &limits);
    assert!(matches!(
        invalid,
        llm::Decoded::Invalid { problem: llm::Problem::Missing { field } } if field.as_ref() == b"text"
    ));
    let cases = [
        br#"{"form":"change","fields":{"ticket":"abc"}}"#.as_slice(),
        br#"{"form":"verdict","label":"approve","text":"looks good","fields":{"source":"test"},"items":[{"kind":"observation","fields":{"path":"a.rs"}}]}"#,
        br#"{"form":"report","text":"done","fields":{"source":"test"}}"#,
        br#"{"form":"failure","reason":"blocked","fields":{"cause":"remote"}}"#,
    ];
    for input in cases {
        let ask = served(adapter::decode_finish(input, &contract, &limits));
        let run::Ask::Finish { outcome } = ask else { panic!("finish ask") };
        assert_eq!(run::outcome::judge(&contract, &outcome), Ok(()), "{}", String::from_utf8_lossy(input));
    }
}

#[test]
fn delivery_fields_decode_without_interpreting_host_names() {
    let limits = limits();
    let change = contract().change.expect("separate change rule");
    let malformed = adapter::decode_deliver(br#"{"fields":{"ticket":12}}"#, &change, &limits);
    assert!(matches!(
        malformed,
        llm::Decoded::Invalid { problem: llm::Problem::WrongType { field } } if field.as_ref() == b"ticket"
    ));
    let ask = served(adapter::decode_deliver(br#"{"fields":{"ticket":"abc","extra":"kept"}}"#, &change, &limits));
    let run::Ask::Deliver { change } = ask else { panic!("delivery ask") };
    assert_eq!(change.fields.len(), 2);
    assert_eq!(change.fields[1].name.as_ref(), b"extra");
    assert_eq!(change.fields[1].value.as_ref(), b"kept");
}

#[test]
fn contract_schemas_reach_the_scripted_byte_peer_in_both_dialects() {
    let bounds = limits();
    let rules = contract();
    for configuration in &wire::configurations() {
        let prompt = llm::Prompt {
            endpoint: llm::Endpoint(1),
            model: b"model".as_slice().into(),
            system: b"@contract-schema".as_slice().into(),
            tools: smith_domain::tools::Grants { inspect: false, modify: false, shell: false },
            served: Box::new([llm::Served::Finish, llm::Served::Deliver]),
            messages: Box::new([llm::Message {
                role: llm::Role::User,
                content: Box::new([llm::Block::Text { text: b"Finish.".as_slice().into(), replay: None }]),
            }]),
            max_tokens: 32,
        };
        let receiving = adapter::Receiving {
            max_completion_bytes: adapter::completion_worst_case(&bounds.client, 4096).expect("bounded completion"),
            max_completion_blocks: bounds.client.dialect.parts,
            decoded_call_bytes: 4096,
            max_failure_bytes: bounds.client.dialect.detail_bytes,
        };
        let prepared = adapter::prepare_for_contract(
            adapter::Input {
                owner: Token::new(91),
                endpoint_name: prompt.endpoint,
                prompt,
                endpoint: configuration.endpoint.clone(),
                credential: skein_llm::Credential {
                    access_token: configuration.credential.access_token.clone(),
                    account_id: configuration.credential.account_id.clone(),
                },
                application: Box::new([]),
                receiving,
            },
            &rules,
            rules.change.as_ref(),
            &bounds,
        )
        .expect("contract schema admitted before provider work");
        let scripts = Box::new([Script {
            cue: b"@contract-schema".as_slice().into(),
            turns: Box::new([Turn {
                lines: Box::new([Line::Text { text: b"ready".as_slice().into() }]),
                finish: Finish::Stop,
                tokens: 1,
            }]),
        }]);
        let mut peer = skein_llm_world::fake::Exchange::prepared(
            prepared.client,
            configuration.endpoint.clone(),
            skein_llm::Credential {
                access_token: configuration.credential.access_token.clone(),
                account_id: configuration.credential.account_id.clone(),
            },
            bounds.client,
            scripts,
        );
        peer.start();
        peer.run();
        let [query] = peer.queries.as_slice() else { panic!("one provider query") };
        assert_eq!(query.tools.len(), 2);
        for offered in &query.tools {
            let schema = skein_llm::Json::from_bytes(&offered.parameters, &bounds.client.dialect)
                .expect("scripted peer accepts the whole schema document");
            assert_eq!(schema.as_tokens().first(), Some(&skein_json::Token::ObjectStart));
        }
        assert!(query.tools[0].parameters.windows(b"observation".len()).any(|part| part == b"observation"));
        assert!(query.tools[1].parameters.windows(b"ticket".len()).any(|part| part == b"ticket"));
    }
}
