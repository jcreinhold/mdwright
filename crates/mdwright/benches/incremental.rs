//! Editing-session bench for `mdwright_format::format_range_with_checkpoints`.
//!
//! Simulates an LSP server formatting on every keystroke: load a 200 KB
//! fixture, build the `CheckpointTable` once, then time 1 000 random
//! ~100-byte range-format calls. Targets per the plan:
//!
//! * median per-edit latency ≤ 1 ms
//! * p99 per-edit latency ≤ 5 ms
//! * full 1 000-edit session ≤ 1.5 s wall
//!
//! The fixture is `benches/fixtures/large.md` — `medium.md × 3 +
//! small.md` concatenated, checked in deterministically so bench
//! numbers are stable across runs.
//!
//! Allocation discipline (per `optimizing-rust-performance` §3): the
//! `checkpoint_table_build` bench measures the steady-state cost of
//! the boundary scan; combined with the unit-test asserting one
//! `Vec::with_capacity` plus at most one `Source` canonical buffer
//! allocation, it pins the invariant that `CheckpointTable::build`
//! makes O(1) allocations regardless of document size.

#![allow(clippy::expect_used, reason = "bench fixtures should fail loudly")]

use std::fs;
use std::hint::black_box;
use std::ops::Range;
use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use mdwright_document::Document;
use mdwright_format::{CheckpointTable, FmtOptions, format_range_with_checkpoints};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load_large_fixture() -> String {
    let path = manifest_dir().join("benches").join("fixtures").join("large.md");
    fs::read_to_string(&path).unwrap_or_else(|e| {
        #[allow(clippy::panic)]
        {
            panic!("bench fixture {} missing: {e}", path.display())
        }
    })
}

/// Deterministic ~100-byte ranges covering the document. Uses a fixed
/// LCG seed so the per-edit set is identical across runs — Criterion's
/// noise floor catches real regressions only if the workload is stable.
fn sample_edit_ranges(source_len: usize, count: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::with_capacity(count);
    let span = 100usize.min(source_len);
    let modulus = source_len.saturating_sub(span).max(1);
    // Numerical Recipes LCG (Knuth/Lewis): cycle length 2^32, fine for
    // sample-position selection.
    let mut x: u32 = 0x1234_5678;
    for _ in 0..count {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let start = (x as usize).checked_rem(modulus).unwrap_or(0);
        let end = (start.saturating_add(span)).min(source_len);
        ranges.push(start..end);
    }
    ranges
}

fn bench_editing_session(c: &mut Criterion) {
    let source = load_large_fixture();
    let doc = Document::parse(&source).expect("fixture parses");
    let opts = FmtOptions::default();
    let table = CheckpointTable::from_document(&doc);
    let edits = sample_edit_ranges(doc.source().len(), 1_000);

    let mut g = c.benchmark_group("incremental");
    // Whole-session throughput: 1000 edits per iteration.
    g.bench_function("session_1000_edits", |b| {
        b.iter(|| {
            for r in &edits {
                let out =
                    format_range_with_checkpoints(black_box(&doc), black_box(&opts), black_box(&table), r.clone());
                black_box(out);
            }
        });
    });
    // Per-edit latency: one format_range call per iteration over a
    // deterministic mid-document range, so Criterion's outlier stats
    // surface tail latency for the LSP's worst-case keystroke.
    let half = doc.source().len().checked_div(2).unwrap_or(0);
    let mid = half..half.saturating_add(100);
    g.bench_function("single_edit_mid", |b| {
        b.iter(|| {
            let out = format_range_with_checkpoints(black_box(&doc), black_box(&opts), black_box(&table), mid.clone());
            black_box(out);
        });
    });
    g.finish();
}

fn bench_table_build(c: &mut Criterion) {
    let doc = load_large_fixture();
    let mut g = c.benchmark_group("incremental");
    g.bench_function("checkpoint_table_build", |b| {
        b.iter(|| {
            let t = CheckpointTable::build(black_box(&doc)).expect("large bench fixture parses");
            black_box(t);
        });
    });
    g.finish();
}

fn bench_all(c: &mut Criterion) {
    bench_table_build(c);
    bench_editing_session(c);
}

criterion_group!(benches, bench_all);
criterion_main!(benches);
