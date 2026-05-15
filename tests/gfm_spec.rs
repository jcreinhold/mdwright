#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test harness; failure modes are aggregated assertions, not panics for users"
)]

//! GFM spec compliance.
//!
//! Vendored cmark-gfm `spec.txt` under `tests/gfm-spec/`. Each example
//! is `parse → format → parse → format → compare`; the runner asserts
//! the formatter is a fixed point of GFM-compliant parsing, and that
//! source and formatted output share both HTML rendering (cheap
//! invariant) and a normalised pulldown-cmark event stream (the
//! stronger invariant that catches silent raw-HTML insertion).
//!
//! Two entry points: [`gfm_spec_fast`] runs a curated subset and is
//! part of the default `cargo test` run; [`gfm_spec_full`] runs every
//! case behind `#[ignore]`. The allowlist at
//! `tests/gfm-spec/known-mismatches.txt` is shared.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use mdwright::{Document, FmtOptions, render_html};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

#[derive(Debug)]
struct SpecCase {
    number: u32,
    section: String,
    class: String,
    source: String,
}

fn spec_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gfm-spec/spec.txt")
}

fn allowlist_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gfm-spec/known-mismatches.txt")
}

fn load_spec() -> Vec<SpecCase> {
    let text = fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec.txt: {e}"));
    parse_spec(&text)
}

fn load_allowlist() -> HashSet<u32> {
    let text = fs::read_to_string(allowlist_path()).unwrap_or_default();
    let mut out = HashSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let token = trimmed.split_whitespace().next().unwrap_or("");
        if let Ok(n) = token.parse::<u32>() {
            out.insert(n);
        }
    }
    out
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
        let Some(class_part) = header_rest.strip_prefix("example") else {
            continue;
        };
        let class = class_part.trim().to_owned();
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
            class,
            source,
        });
    }
    out
}

fn strip_anchor(s: &str) -> String {
    s.split(" {#").next().unwrap_or(s).trim().to_owned()
}

fn parser_options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
}

/// Strings-only normalisation of a pulldown-cmark event stream. Drops
/// byte ranges and `CowStr` provenance so structurally-equivalent
/// streams from different sources compare equal. The level of detail
/// is deliberately coarse — we want to catch *meaning* changes
/// (insertion of HTML, dropped emphasis, structural divergence), not
/// punish cosmetic differences in how text is chunked.
fn ast_events(source: &str) -> Vec<String> {
    let parser = Parser::new_ext(source, parser_options());
    let mut out = Vec::new();
    for ev in parser {
        out.push(normalise_event(ev));
    }
    out
}

fn normalise_event(ev: Event<'_>) -> String {
    match ev {
        Event::Start(tag) => format!("Start({})", normalise_tag(&tag)),
        Event::End(tag) => format!("End({})", normalise_tag_end(tag)),
        Event::Text(s) => format!("Text({})", s.as_ref()),
        Event::Code(s) => format!("Code({})", s.as_ref()),
        Event::Html(s) => format!("Html({})", s.as_ref()),
        Event::InlineHtml(s) => format!("InlineHtml({})", s.as_ref()),
        Event::FootnoteReference(s) => format!("FootnoteReference({})", s.as_ref()),
        Event::SoftBreak => "SoftBreak".to_owned(),
        Event::HardBreak => "HardBreak".to_owned(),
        Event::Rule => "Rule".to_owned(),
        Event::TaskListMarker(b) => format!("TaskListMarker({b})"),
        Event::InlineMath(s) => format!("InlineMath({})", s.as_ref()),
        Event::DisplayMath(s) => format!("DisplayMath({})", s.as_ref()),
    }
}

fn normalise_tag(tag: &Tag<'_>) -> String {
    match tag {
        Tag::Paragraph => "Paragraph".to_owned(),
        Tag::Heading { level, .. } => format!("Heading({level:?})"),
        Tag::BlockQuote(_) => "BlockQuote".to_owned(),
        Tag::CodeBlock(kind) => format!("CodeBlock({kind:?})"),
        Tag::List(start) => format!("List({start:?})"),
        Tag::Item => "Item".to_owned(),
        Tag::FootnoteDefinition(label) => format!("FootnoteDefinition({})", label.as_ref()),
        Tag::Table(alignments) => format!("Table({alignments:?})"),
        Tag::TableHead => "TableHead".to_owned(),
        Tag::TableRow => "TableRow".to_owned(),
        Tag::TableCell => "TableCell".to_owned(),
        Tag::Emphasis => "Emphasis".to_owned(),
        Tag::Strong => "Strong".to_owned(),
        Tag::Strikethrough => "Strikethrough".to_owned(),
        Tag::Link {
            link_type,
            dest_url,
            title,
            ..
        } => format!("Link({link_type:?},{},{})", dest_url.as_ref(), title.as_ref()),
        Tag::Image {
            link_type,
            dest_url,
            title,
            ..
        } => format!("Image({link_type:?},{},{})", dest_url.as_ref(), title.as_ref()),
        Tag::HtmlBlock => "HtmlBlock".to_owned(),
        Tag::MetadataBlock(kind) => format!("MetadataBlock({kind:?})"),
        Tag::DefinitionList => "DefinitionList".to_owned(),
        Tag::DefinitionListTitle => "DefinitionListTitle".to_owned(),
        Tag::DefinitionListDefinition => "DefinitionListDefinition".to_owned(),
        Tag::Subscript => "Subscript".to_owned(),
        Tag::Superscript => "Superscript".to_owned(),
    }
}

fn normalise_tag_end(tag: TagEnd) -> String {
    format!("{tag:?}")
}

#[derive(Debug, Default, Clone)]
struct CaseFailure {
    case: u32,
    section: String,
    class: String,
    kind: &'static str,
    detail: String,
}

fn run_case(case: &SpecCase) -> Vec<CaseFailure> {
    let mut fails = Vec::new();
    let opts = FmtOptions::default();
    let formatted = Document::parse(&case.source).format(&opts);

    // Idempotence.
    let refmt = Document::parse(&formatted).format(&opts);
    if refmt != formatted {
        fails.push(CaseFailure {
            case: case.number,
            section: case.section.clone(),
            class: case.class.clone(),
            kind: "idempotence",
            detail: format!(
                "format(format(x)) != format(x)\nsource:\n{}---\nformatted:\n{formatted}---\nrefmt:\n{refmt}---",
                case.source
            ),
        });
        // If the formatter is not idempotent on this case, the HTML /
        // AST comparisons are noise — bail.
        return fails;
    }

    let src_html = render_html(&case.source);
    let fmt_html = render_html(&formatted);
    if src_html != fmt_html {
        fails.push(CaseFailure {
            case: case.number,
            section: case.section.clone(),
            class: case.class.clone(),
            kind: "html",
            detail: format!(
                "HTML differs:\nsource:\n{}---\nformatted:\n{formatted}---\nsource_html:\n{src_html}\nformatted_html:\n{fmt_html}",
                case.source
            ),
        });
    }

    let src_ast = ast_events(&case.source);
    let fmt_ast = ast_events(&formatted);
    if src_ast != fmt_ast {
        fails.push(CaseFailure {
            case: case.number,
            section: case.section.clone(),
            class: case.class.clone(),
            kind: "ast",
            detail: format!(
                "AST event streams differ:\nsource:\n{}---\nformatted:\n{formatted}---\nsource_events:\n{src_ast:#?}\nformatted_events:\n{fmt_ast:#?}",
                case.source
            ),
        });
    }
    fails
}

fn report(fails: &[CaseFailure], total: usize) -> String {
    let mut out = format!("{} failure(s) across {total} case(s):\n", fails.len());
    for f in fails {
        let _ = write!(
            out,
            "\n--- case {} ({}, {}) [{}] ---\n{}\n",
            f.case,
            if f.section.is_empty() { "?" } else { f.section.as_str() },
            if f.class.is_empty() { "core" } else { f.class.as_str() },
            f.kind,
            f.detail,
        );
    }
    out
}

/// Curated case numbers that currently pass under both invariants.
/// These are regression sentinels — adding a case here requires that
/// it pass `gfm_spec_full` first. Categories represented: emphasis,
/// links, images, autolinks, tables, strikethrough, task lists, and a
/// handful of core `CommonMark` constructs. Kept small so
/// `cargo test --release` stays under a few seconds.
const FAST_CASES: &[u32] = &[
    32, 50, 62, 181, 198, 199, 279, 282, 351, 424, 490, 491, 492, 537, 584, 602, 620, 632, 634,
];

/// Baseline failure count for the full spec sweep. The exhaustive
/// sweep asserts `failures ≤ FULL_BASELINE_FAILURES`: regressions
/// fail the test, improvements pass with room to lower the baseline
/// on the next commit. The current value reflects mdwright's
/// formatter as of Phase 3 landing — substantial gaps remain (notably
/// raw-HTML round-tripping, list-item idempotence, and edge cases in
/// emphasis nesting). Future sessions tighten this number toward zero.
const FULL_BASELINE_FAILURES: usize = 243;

fn run_subset(cases: &[SpecCase], allow: &HashSet<u32>, fast: bool) -> (Vec<CaseFailure>, usize) {
    let mut fails = Vec::new();
    let mut count = 0usize;
    let selector: Box<dyn Fn(&SpecCase) -> bool> = if fast {
        let set: HashSet<u32> = FAST_CASES.iter().copied().collect();
        Box::new(move |c| set.contains(&c.number))
    } else {
        Box::new(|_| true)
    };
    for case in cases {
        if !selector(case) {
            continue;
        }
        if allow.contains(&case.number) {
            continue;
        }
        count = count.saturating_add(1);
        fails.extend(run_case(case));
    }
    (fails, count)
}

#[test]
fn gfm_spec_fast() {
    let cases = load_spec();
    let allow = load_allowlist();
    let (fails, count) = run_subset(&cases, &allow, true);
    assert!(count > 0, "fast subset is empty — check FAST_CASES");
    assert!(fails.is_empty(), "{}", report(&fails, count));
}

/// Exhaustive sweep. Asserts the failure count does not exceed
/// [`FULL_BASELINE_FAILURES`]; the absolute count is informational.
/// `#[ignore]`d by default — run with
/// `cargo test --release -- --ignored gfm_spec_full`.
#[test]
#[ignore = "full GFM spec sweep; run with `cargo test --release -- --ignored gfm_spec_full`"]
fn gfm_spec_full() {
    let cases = load_spec();
    let allow = load_allowlist();
    let (fails, count) = run_subset(&cases, &allow, false);
    assert!(count > 0, "spec parse produced no cases");
    let unique_cases: HashSet<u32> = fails.iter().map(|f| f.case).collect();
    let n = unique_cases.len();
    eprintln!("gfm_spec_full: {n} cases failed across {count} total");
    assert!(
        n <= FULL_BASELINE_FAILURES,
        "{n} failing cases; baseline is {FULL_BASELINE_FAILURES}. \
         A regression has been introduced. Sample failures:\n{}",
        report(&fails.iter().take(10).cloned().collect::<Vec<_>>(), count)
    );
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
