#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test harness; failure modes are aggregated assertions, not panics for users"
)]

//! GFM spec conformance, as a snapshot.
//!
//! Vendored cmark-gfm `spec.txt` lives under `tests/gfm-spec/`. Each
//! example is run through `parse → format → parse → format` and
//! compared against the source's HTML and normalised event stream.
//! Phase R retired the ratchet:
//!
//! * [`gfm_spec_snapshot`] runs every case, collects the residual
//!   `(case, kind)` failures *not* covered by the editorial
//!   allowlist, and asserts they match `tests/gfm-spec/snapshot.txt`
//!   byte-for-byte. Any drift — regression *or* improvement — fails
//!   the test with a diff-friendly message. Regenerate with
//!   `MDWRIGHT_UPDATE_SNAPSHOT=1 cargo test --release --test gfm_spec gfm_spec_snapshot`.
//! * [`gfm_spec_coverage`] prints a three-line coverage report and
//!   asserts that every spec case is accounted for as either
//!   `matching`, `intentional deviation` (allowlist), or `tracked
//!   regression` (snapshot).
//!
//! The user-visible contract is exercised at proptest scale in
//! `tests/properties.rs`; this file is the *index* of where the
//! formatter still drifts from the spec, not the contract itself.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use mdwright::{Document, FmtOptions, semantically_equivalent};
use serde::Deserialize;

#[derive(Debug)]
struct SpecCase {
    number: u32,
    section: String,
    source: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gfm-spec/spec.txt")
}

fn allowlist_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gfm-spec/allowlist.toml")
}

fn snapshot_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gfm-spec/snapshot.txt")
}

fn load_spec() -> Vec<SpecCase> {
    let text = fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec.txt: {e}"));
    parse_spec(&text)
}

/// Per-case structured editorial deviation. The allowlist is the
/// curated list of spec cases whose divergence we accept as a
/// formatting choice, not as a bug; each entry documents *why* and
/// where to read more.
#[derive(Debug, Deserialize)]
struct AllowEntry {
    number: u32,
    #[allow(dead_code)]
    bucket: String,
    #[allow(dead_code)]
    reason: String,
    #[allow(dead_code)]
    docs: String,
}

#[derive(Debug, Deserialize)]
struct AllowFile {
    #[serde(default, rename = "case")]
    cases: Vec<AllowEntry>,
}

fn load_allowlist() -> Vec<AllowEntry> {
    let text = fs::read_to_string(allowlist_path()).unwrap_or_default();
    if text.trim().is_empty() {
        return Vec::new();
    }
    let parsed: AllowFile = toml::from_str(&text).unwrap_or_else(|e| panic!("parse allowlist.toml: {e}"));
    parsed.cases
}

/// Spec-example block syntax: a 32-backtick fence opens with the
/// `example` tag (optionally followed by a class like `table` or
/// `autolink`), a `.` separator marks the source/HTML boundary, and a
/// bare 32-backtick fence closes the block. Tabs in source are
/// escaped as `→` (U+2192) and must be decoded back. Section headers
/// (`# … {#anchor}` or `## …`) preceding a block become its `section`
/// label; this is purely informational. Example numbers count
/// upward across the whole file.
fn parse_spec(text: &str) -> Vec<SpecCase> {
    const FENCE: &str = "````````````````````````````````";
    let mut out = Vec::new();
    let mut section = String::new();
    let mut number: u32 = 0;
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_end();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            section = strip_anchor(rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            section = strip_anchor(rest);
            continue;
        }
        let Some(header_rest) = trimmed.strip_prefix(FENCE) else {
            continue;
        };
        let header_rest = header_rest.trim_start();
        if header_rest.strip_prefix("example").is_none() {
            continue;
        }
        number = number.saturating_add(1);

        let mut source = String::new();
        let mut in_source = true;
        for inner in lines.by_ref() {
            let inner_trim = inner.trim_end();
            if inner_trim == FENCE {
                break;
            }
            if in_source {
                if inner_trim == "." {
                    in_source = false;
                    continue;
                }
                source.push_str(&inner.replace('→', "\t"));
                source.push('\n');
            }
            // HTML side is intentionally discarded — the runner uses
            // our own parser on both sides.
        }
        out.push(SpecCase {
            number,
            section: section.clone(),
            source,
        });
    }
    out
}

fn strip_anchor(s: &str) -> String {
    s.split(" {#").next().unwrap_or(s).trim().to_owned()
}

/// Kind labels are part of the snapshot file's stable surface; the
/// order here doubles as the sort order when multiple kinds fail for
/// one case.
const KIND_IDEMPOTENCE: &str = "idempotence";
const KIND_SEMANTIC: &str = "semantic";

#[tracing::instrument(level = "info", name = "run_case", skip(case), fields(case = case.number, section = %case.section))]
fn run_case(case: &SpecCase) -> Vec<&'static str> {
    let mut kinds = Vec::new();
    let opts = FmtOptions::default();
    let formatted = Document::parse(&case.source).format(&opts);

    let refmt = Document::parse(&formatted).format(&opts);
    if refmt != formatted {
        kinds.push(KIND_IDEMPOTENCE);
        // If the formatter is not idempotent on this case, the
        // semantic comparison below is noise — bail.
        return kinds;
    }

    if !semantically_equivalent(&case.source, &formatted) {
        kinds.push(KIND_SEMANTIC);
    }
    kinds
}

/// Collected per-case results: `case_number → (section, [failing kinds])`.
/// `BTreeMap` for deterministic snapshot order.
fn collect_failures(cases: &[SpecCase]) -> BTreeMap<u32, (String, Vec<&'static str>)> {
    let mut out = BTreeMap::new();
    for case in cases {
        let kinds = run_case(case);
        if !kinds.is_empty() {
            out.insert(case.number, (case.section.clone(), kinds));
        }
    }
    out
}

/// One-line-per-failing-kind format. Stable, sortable, diff-friendly.
fn render_snapshot(failures: &BTreeMap<u32, (String, Vec<&'static str>)>, allowlist: &BTreeSet<u32>) -> String {
    let mut out = String::from(
        "# GFM spec snapshot. Auto-generated; one line per (case, kind) failure.\n\
         # Regenerate after a deliberate fix with:\n\
         #   MDWRIGHT_UPDATE_SNAPSHOT=1 cargo test --release --test gfm_spec gfm_spec_snapshot\n\
         # Allowlisted (editorial deviation) cases are filtered out — see allowlist.toml.\n",
    );
    for (num, (section, kinds)) in failures {
        if allowlist.contains(num) {
            continue;
        }
        let section = if section.is_empty() { "?" } else { section.as_str() };
        for kind in kinds {
            let _ = writeln!(out, "{num:<5} {kind:<11} {section}");
        }
    }
    out
}

#[test]
fn spec_parses_to_nonempty() {
    let cases = load_spec();
    assert!(
        cases.len() >= 600,
        "spec parse produced only {} cases; expected ~670",
        cases.len()
    );
}

#[test]
fn allowlist_is_well_formed() {
    let cases = load_spec();
    let valid: HashSet<u32> = cases.iter().map(|c| c.number).collect();
    let entries = load_allowlist();
    let mut seen = HashSet::new();
    for entry in &entries {
        assert!(
            valid.contains(&entry.number),
            "allowlist references case {} which is not in the spec",
            entry.number
        );
        assert!(
            seen.insert(entry.number),
            "allowlist has a duplicate entry for case {}",
            entry.number
        );
        assert!(
            !entry.bucket.is_empty(),
            "allowlist entry for case {} has empty bucket",
            entry.number
        );
        assert!(
            !entry.reason.is_empty(),
            "allowlist entry for case {} has empty reason",
            entry.number
        );
        assert!(
            !entry.docs.is_empty(),
            "allowlist entry for case {} has empty docs",
            entry.number
        );
    }
}

/// Snapshot of all currently-failing non-allowlisted spec cases. Any
/// drift — regression *or* improvement — fails the test. To rebaseline
/// after a deliberate change, set `MDWRIGHT_UPDATE_SNAPSHOT=1`.
#[test]
fn gfm_spec_snapshot() {
    let cases = load_spec();
    let allowlist: BTreeSet<u32> = load_allowlist().iter().map(|e| e.number).collect();
    let failures = collect_failures(&cases);
    let actual = render_snapshot(&failures, &allowlist);
    let path = snapshot_path();

    if std::env::var_os("MDWRIGHT_UPDATE_SNAPSHOT").is_some() {
        fs::write(&path, &actual).unwrap_or_else(|e| panic!("write snapshot: {e}"));
        eprintln!("snapshot rewritten at {}", path.display());
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_default();
    if actual != expected {
        let diff = unified_diff(&expected, &actual);
        panic!(
            "GFM spec snapshot drift at {}.\n\
             Rebaseline with: MDWRIGHT_UPDATE_SNAPSHOT=1 cargo test --release --test gfm_spec gfm_spec_snapshot\n\
             \n--- expected (snapshot.txt)\n+++ actual (current run)\n{diff}",
            path.display(),
        );
    }
}

/// Three-line coverage report. Asserts that every spec case is
/// accounted for as one of: matches the formatter exactly,
/// intentionally deviates (allowlist), or is a tracked regression
/// (snapshot). `unexpected == 0` is the live invariant.
#[test]
fn gfm_spec_coverage() {
    let cases = load_spec();
    let allowlist: BTreeSet<u32> = load_allowlist().iter().map(|e| e.number).collect();
    let failing_cases: BTreeSet<u32> = collect_failures(&cases).keys().copied().collect();
    let total = cases.len();
    let intentional = allowlist.len();
    let tracked = failing_cases.difference(&allowlist).count();
    let matching = total.saturating_sub(intentional).saturating_sub(tracked);
    let unexpected = failing_cases
        .iter()
        .filter(|n| !allowlist.contains(n))
        .filter(|n| {
            // A case is "unexpected" only if it isn't already recorded
            // in the snapshot. The snapshot is the ratchet on
            // *known* regressions; `unexpected` is reserved for
            // cases that escape both lists.
            !snapshot_records(**n)
        })
        .count();

    eprintln!(
        "GFM spec coverage:\n  total cases:        {total}\n  fully matching:     {matching}\n  intentional dev:    {intentional}\n  tracked regression: {tracked}\n  unexpected:         {unexpected}"
    );
    assert_eq!(
        unexpected, 0,
        "{unexpected} cases failed without being in the snapshot or allowlist"
    );
}

/// True if the snapshot file contains a failure line for `case`.
/// Reads the snapshot from disk once per call; coverage runs in
/// `O(snapshot_size)` which is fine for a few hundred lines.
fn snapshot_records(case: u32) -> bool {
    let Ok(text) = fs::read_to_string(snapshot_path()) else {
        return false;
    };
    let prefix = format!("{case} ");
    text.lines().any(|l| {
        let trimmed = l.trim_start();
        trimmed.starts_with(&prefix) || trimmed.starts_with(&format!("{case:<5}"))
    })
}

/// Bare-bones line-by-line diff for the snapshot drift message; we
/// keep this in-crate to avoid pulling in a diff dep just for tests.
fn unified_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut out = String::new();
    let max = old_lines.len().max(new_lines.len());
    for i in 0..max {
        match (old_lines.get(i), new_lines.get(i)) {
            (Some(a), Some(b)) if a == b => {
                let _ = writeln!(out, " {a}");
            }
            (Some(a), Some(b)) => {
                let _ = writeln!(out, "-{a}");
                let _ = writeln!(out, "+{b}");
            }
            (Some(a), None) => {
                let _ = writeln!(out, "-{a}");
            }
            (None, Some(b)) => {
                let _ = writeln!(out, "+{b}");
            }
            (None, None) => {}
        }
    }
    out
}
