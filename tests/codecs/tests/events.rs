//! The event vocabulary round trips exact integers and filters capture as specified.

#[path = "support/events.rs"]
mod fixtures;

use smith_events::{Capture, Error, Event, Limits, read, write};

fn limits() -> Limits {
    Limits { string: 64, content: 1024, items: 8 }
}

#[test]
fn every_record_and_listed_value_reads_back_to_its_written_values() {
    for (name, record, capture) in fixtures::samples() {
        let line = write(&record, &capture, &limits()).expect(name).expect("a recorded event");
        let decoded = read(&line, &limits()).expect(name).expect("a known record");
        if capture == Capture::Everything {
            assert_eq!(decoded, record, "{name}");
        }
        let rewritten = write(&decoded, &capture, &limits()).expect(name).expect("known record");
        assert_eq!(rewritten, line, "{name}");
        assert!(
            line.len()
                <= usize::try_from(smith_events::largest_size(&record.event, &capture, &limits()).expect("bound"))
                    .expect("usize")
        );
    }
}

#[test]
fn capture_filters_only_content_fields_and_live_text() {
    let records = fixtures::samples();
    for (_, record, _) in records {
        let none = write(&record, &Capture::None, &limits()).expect("bounded");
        match &record.event {
            Event::TextDelta(_) => assert!(none.is_none()),
            Event::ResponseCompleted(_) => {
                let line = none.expect("metadata");
                match read(&line, &limits()).expect("read").expect("event").event {
                    Event::ResponseCompleted(value) => assert!(value.completion.is_none()),
                    Event::SessionStarted(_)
                    | Event::SessionEnded(_)
                    | Event::RunStarted(_)
                    | Event::RunCompleted(_)
                    | Event::ConversationOpened(_)
                    | Event::ConversationClosed(_)
                    | Event::ResponseStarted(_)
                    | Event::TextDelta(_)
                    | Event::ToolStarted(_)
                    | Event::ToolCompleted(_)
                    | Event::CheckStarted(_)
                    | Event::CheckCompleted(_)
                    | Event::Notice(_) => panic!("response"),
                }
            }
            Event::SessionStarted(_)
            | Event::SessionEnded(_)
            | Event::RunStarted(_)
            | Event::RunCompleted(_)
            | Event::ConversationOpened(_)
            | Event::ConversationClosed(_)
            | Event::ResponseStarted(_)
            | Event::ToolStarted(_)
            | Event::ToolCompleted(_)
            | Event::CheckStarted(_)
            | Event::CheckCompleted(_)
            | Event::Notice(_) => {
                let line = String::from_utf8(none.expect("metadata").into_vec()).expect("UTF-8");
                for key in ["\"prompt\":", "\"input\":", "\"text\":", "\"fields\":"] {
                    assert!(!line.contains(key), "content removed under none: {key}");
                }
            }
        }
    }
}

#[test]
fn unknown_types_fields_and_listed_values_are_tolerated_within_this_version() {
    let limits = limits();
    assert_eq!(read(b"{\"v\":1,\"type\":\"future\",\"t_ms\":0,\"extra\":[{}]}\n", &limits), Ok(None));
    let (_, record, capture) = fixtures::samples()
        .into_iter()
        .find(|(_, record, _)| matches!(record.event, Event::Notice(_)))
        .expect("notice");
    let line = write(&record, &capture, &limits).expect("write").expect("record");
    let text = String::from_utf8(line.into_vec())
        .expect("UTF8")
        .replace("\"warning\"", "\"future\"")
        .replace("\"info\"", "\"future\"");
    let text = text.trim_end().strip_suffix('}').expect("object").to_owned() + ",\"extra\":{\"v\":9}}\n";
    let decoded = read(text.as_bytes(), &limits).expect("forward values").expect("notice");
    match decoded.event {
        Event::Notice(notice) => {
            assert_eq!(notice.level, smith_events::Level::Unknown(Box::from(b"future".as_slice())));
        }
        Event::SessionStarted(_)
        | Event::SessionEnded(_)
        | Event::RunStarted(_)
        | Event::RunCompleted(_)
        | Event::ConversationOpened(_)
        | Event::ConversationClosed(_)
        | Event::ResponseStarted(_)
        | Event::ResponseCompleted(_)
        | Event::TextDelta(_)
        | Event::ToolStarted(_)
        | Event::ToolCompleted(_)
        | Event::CheckStarted(_)
        | Event::CheckCompleted(_) => panic!("notice"),
    }
    assert_eq!(read(b"{\"v\":99,\"type\":\"future\",\"t_ms\":0}", &limits), Err(Error::Version(99)));
}

#[test]
fn malformed_known_shapes_and_overflowing_integers_are_refused() {
    for line in [
        b"{\"v\":1,\"v\":1,\"type\":\"notice\",\"t_ms\":0}".as_slice(),
        b"{\"v\":1,\"type\":\"notice\",\"t_ms\":18446744073709551616}",
        b"{\"v\":1,\"type\":\"notice\",\"t_ms\":-1}",
        b"{\"v\":1,\"type\":\"notice\",\"t_ms\":1.5}",
    ] {
        assert!(read(line, &limits()).is_err());
    }
}

#[test]
fn a_chunked_stream_checks_each_version_and_rejects_an_unterminated_tail() {
    let limits = limits();
    let (_, value, capture) = fixtures::samples().remove(0);
    let line = write(&value, &capture, &limits).expect("line").expect("record");
    let mut reader = smith_events::Reader::new(&limits).expect("bounded reader");
    for byte in line.as_ref() {
        let result = reader.feed(&[*byte]).expect("one byte");
        assert_eq!(result.consumed, 1);
        assert_eq!(result.complete, *byte == b'\n');
        if let Some(record) = result.record {
            assert_eq!(record.t_ms, u64::MAX);
        }
    }
    reader.finish().expect("whole stream");
    let unknown = reader.feed(b"{\"v\":1,\"type\":\"future\",\"t_ms\":0}\ntrailing").expect("skip unknown line");
    assert!(unknown.complete && unknown.record.is_none());
    assert_eq!(
        reader.feed(b"{\"v\":2,\"type\":\"future\",\"t_ms\":0}\n").expect_err("stream version checked"),
        Error::Version(2)
    );
    reader.feed(b"{").expect("partial");
    assert_eq!(reader.finish(), Err(Error::Malformed));
}

#[test]
fn a_frozen_line_of_another_version_is_refused_by_name() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-events/golden/unsupported-v2.jsonl");
    let line = std::fs::read(path).expect("unsupported golden");
    assert_eq!(read(&line, &limits()), Err(Error::Version(2)));
}

#[test]
fn configured_boundaries_and_unknown_content_kinds_are_checked() {
    let (_, mut record, _) = fixtures::samples()
        .into_iter()
        .find(|(_, value, _)| matches!(value.event, Event::TextDelta(_)))
        .expect("delta");
    let limits = Limits { string: 32, content: 4, items: 2 };
    if let Event::TextDelta(value) = &mut record.event {
        value.text = Box::from(b"1234".as_slice());
    }
    let line = write(&record, &Capture::Everything, &limits).expect("exact content bound").expect("line");
    assert_eq!(read(&line, &limits).expect("reads"), Some(record.clone()));
    if let Event::TextDelta(value) = &mut record.event {
        value.text = Box::from(b"12345".as_slice());
    }
    assert_eq!(write(&record, &Capture::Everything, &limits), Err(Error::Limits));
    if let Event::TextDelta(value) = &mut record.event {
        value.text = Box::from([255_u8].as_slice());
    }
    assert_eq!(write(&record, &Capture::Everything, &limits), Err(Error::Text));
    let overflowing = Limits { string: u32::MAX, content: u32::MAX, items: u32::MAX };
    assert_eq!(smith_events::largest_record(&overflowing), None);
    assert_eq!(smith_events::worst_case(&overflowing), None);
    let (_, prompt, _) = fixtures::samples()
        .into_iter()
        .find(|(_, value, capture)| matches!(value.event, Event::ResponseStarted(_)) && *capture == Capture::Everything)
        .expect("prompt");
    let line = write(&prompt, &Capture::Everything, &self::limits()).expect("prompt").expect("line");
    let future = String::from_utf8(line.into_vec()).expect("UTF8").replace("\"type\":\"text\"", "\"type\":\"future\"");
    assert!(read(future.as_bytes(), &self::limits()).expect("unknown block kind").is_some());
}

#[test]
fn a_reasoning_drop_notice_keeps_its_size_under_every_capture() {
    let frozen = include_bytes!("../../../crates/smith-events/golden/v1/notice_kind_reasoning_dropped.jsonl");
    let record = read(frozen, &limits()).expect("drop notice").expect("known event");
    for capture in [Capture::None, Capture::Calls, Capture::Everything] {
        assert_eq!(write(&record, &capture, &limits()).expect("write").expect("notice").as_ref(), frozen);
    }
    assert_eq!(
        read(br#"{"v":1,"type":"notice","t_ms":0,"level":"warning","kind":"reasoning_dropped"}"#, &limits()),
        Err(Error::Shape)
    );
}
