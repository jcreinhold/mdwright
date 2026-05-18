//! End-to-end parse + format benchmark for the LSP hot path.
//!
//! The LSP server's per-keystroke debounced lint runs `Document::parse`
//! and the formatter on every change. This bench measures the round-trip
//! on a ~6.9 k-line synthetic input (`large.md` + `medium.md` + `large.md`)
//! to guard the < 100 ms editor-latency budget the LSP advertises.
//!
//! Cold paths (file I/O, config discovery, diagnostic pretty-printing,
//! JSON serialisation) stay outside `b.iter()` — only the steady-state
//! parse + format work is measured.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use mdwright_document::Document;
use mdwright_format::FmtOptions;

fn parse_format(c: &mut Criterion) {
    let large = include_str!("fixtures/large.md");
    let medium = include_str!("fixtures/medium.md");
    let mut buf = String::with_capacity(
        large
            .len()
            .saturating_mul(2)
            .saturating_add(medium.len())
            .saturating_add(16),
    );
    buf.push_str(large);
    if !buf.ends_with('\n') {
        buf.push('\n');
    }
    buf.push_str(medium);
    if !buf.ends_with('\n') {
        buf.push('\n');
    }
    buf.push_str(large);
    let opts = FmtOptions::default();
    let line_count = buf.bytes().filter(|&b| b == b'\n').count();
    let id = format!("parse_format/{line_count}lines");

    let mut group = c.benchmark_group("parse_format");
    group.bench_function(id, |b| {
        b.iter(|| {
            let doc = Document::parse(black_box(buf.as_str()));
            black_box(mdwright_format::format_document(&doc, black_box(&opts)))
        });
    });
    group.finish();
}

criterion_group!(benches, parse_format);
criterion_main!(benches);
