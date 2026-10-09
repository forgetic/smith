//! Rendering properties with deterministic input and the ordinary Rust test kit.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "testing-strategy.md, section 4: test harnesses are ordinary Rust"
)]

use skein_lib::{Duration, Rng};
use smith_domain::tools;

use crate::{CUT_MARKER_BYTES, render_outcome, render_worst_case};

fn tools() -> tools::Limits {
    tools::Limits {
        kits: 1,
        calls: 1,
        repos: 1,
        path_bytes: 32,
        known_files: 1,
        file_bytes: 1024,
        read_bytes: 64,
        list_entries: 8,
        list_bytes: 512,
        match_lines: 8,
        file_timeout: Duration::from_secs(30),
        env_bytes: 32,
        shell_timeout: Duration::from_secs(30),
        shell_timeout_max: Duration::from_secs(120),
        shell_head: 64,
        shell_tail: 64,
        search_hits: 8,
        search_bytes: 128,
        search_timeout: Duration::from_secs(30),
        facts: 32,
    }
}

#[test]
fn random_outcomes_at_random_caps_fit_their_derived_render_bound() {
    let mut rng = Rng::new(941);
    for _case in 0_usize..256 {
        let mut caps = tools();
        caps.read_bytes = u32::try_from(rng.next_u64() % 1024).unwrap();
        caps.shell_head = u32::try_from(rng.next_u64() % 1024).unwrap();
        caps.shell_tail = u32::try_from(rng.next_u64() % 1024).unwrap();
        caps.search_hits = u32::try_from(rng.next_u64() % 32).unwrap();
        caps.search_bytes = u32::try_from(rng.next_u64() % 1024).unwrap();
        let bound = render_worst_case(&caps, 1024, 2048).unwrap();
        let entries = (0..caps.list_entries)
            .map(|_| tools::Entry {
                name: tools::Name::new(vec![0xff; 8].into()).unwrap(),
                kind: tools::Kind::Directory,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let hits = (0..caps.search_hits)
            .map(|_| tools::Hit { path: Box::new([]), line: u32::MAX, text: Box::new([]) })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let cases = [
            tools::Outcome::Read {
                content: vec![b'\n'; usize::try_from(caps.read_bytes).unwrap()].into(),
                skipped: u32::MAX,
                lines: u32::MAX,
                total: u32::MAX,
                cut: true,
            },
            tools::Outcome::Listed { entries, more: u64::MAX },
            tools::Outcome::Found { hits, more: u64::MAX, timed_out: true },
            tools::Outcome::Ambiguous {
                count: u32::MAX,
                lines: vec![u32::MAX; usize::try_from(caps.match_lines).unwrap()].into(),
            },
            tools::Outcome::Exited {
                exit: tools::Exit::Code { code: u8::MAX },
                head: vec![0xff; usize::try_from(caps.shell_head).unwrap()].into(),
                tail: vec![0; usize::try_from(caps.shell_tail).unwrap()].into(),
                dropped: u64::MAX,
            },
        ];
        for outcome in cases {
            let (full, _) = render_outcome(&outcome, bound).unwrap();
            assert!(full.len() <= usize::try_from(bound).unwrap());
            assert!(!full.ends_with(b" bytes omitted]"));
        }
    }
}

#[test]
fn a_cut_marker_counts_the_exact_bytes_left_out_of_the_rendering() {
    let outcome = tools::Outcome::Read { content: vec![b'x'; 512].into(), skipped: 0, lines: 1, total: 1, cut: false };
    let (full, error) = render_outcome(&outcome, 4096).unwrap();
    for cap in CUT_MARKER_BYTES..128 {
        let (cut, cut_error) = render_outcome(&outcome, cap).unwrap();
        let marker = cut.windows(2).position(|bytes| bytes == b"\n[").unwrap();
        let count =
            core::str::from_utf8(&cut[marker + 2..]).unwrap().split(' ').next().unwrap().parse::<usize>().unwrap();
        assert_eq!(count, full.len() - marker);
        assert_eq!(&cut[..marker], &full[..marker]);
        assert_eq!(cut_error, error);
        assert!(cut.len() <= usize::try_from(cap).unwrap());
    }
}

#[test]
fn cuts_preserve_every_utf8_character_boundary() {
    let original = "é界🦀".repeat(30);
    for cap in CUT_MARKER_BYTES..128 {
        let cut = crate::render::cut_text(original.as_bytes().into(), cap);
        let text = core::str::from_utf8(&cut).unwrap();
        let (prefix, marker) = text.split_once("\n[").unwrap();
        let omitted = marker.split(' ').next().unwrap().parse::<usize>().unwrap();
        assert!(original.starts_with(prefix));
        assert_eq!(omitted, original.len() - prefix.len());
        assert!(cut.len() <= usize::try_from(cap).unwrap());
    }
}

#[test]
fn rendering_derivation_refuses_arithmetic_overflow() {
    let mut caps = tools();
    caps.list_bytes = u64::MAX;
    assert_eq!(render_worst_case(&caps, 1, 1), None);
}
