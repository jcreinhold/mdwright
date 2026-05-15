//! Criterion benches for `mdwright` parse and lint.
//!
//! Three workloads (small / medium / corpus), three rule sets
//! (none / defaults / all), two operations (parse-only, lint).
//! Numbers from these benches are the baseline against which the
//! formatter sessions are measured.
//!
//! Qualitative reference: `mdformat` on the same corpus is
//! single-digit seconds; lint on this crate should land in the tens
//! of milliseconds. Numbers are machine-local; do not commit them.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};
use mdwright::{Document, RuleSet};
use rayon::prelude::*;
use std::hint::black_box;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    // tools/mdwright/ -> tools/ -> repo root
    let mut p = manifest_dir();
    p.pop();
    p.pop();
    p
}

fn read_to_string(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

fn load_fixture(name: &str) -> String {
    let path = manifest_dir().join("benches").join("fixtures").join(name);
    read_to_string(&path).unwrap_or_else(|e| {
        // Acceptable panic: bench fixtures are committed alongside
        // this file; a missing fixture is a build-tree problem the
        // bench cannot proceed past.
        #[allow(clippy::panic)]
        {
            panic!("bench fixture {} missing: {e}", path.display())
        }
    })
}

fn load_corpus() -> Vec<String> {
    let list_path = manifest_dir().join("benches").join("corpus.list");
    let list = read_to_string(&list_path).unwrap_or_else(|e| {
        #[allow(clippy::panic)]
        {
            panic!("corpus list {} missing: {e}", list_path.display())
        }
    });
    let root = repo_root();
    list.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|rel| {
            let full = root.join(rel);
            read_to_string(&full).unwrap_or_else(|e| {
                #[allow(clippy::panic)]
                {
                    panic!("corpus file {} missing: {e}", full.display())
                }
            })
        })
        .collect()
}

fn bench_parse(c: &mut Criterion) {
    let small = load_fixture("small.md");
    let medium = load_fixture("medium.md");

    let mut g = c.benchmark_group("parse");
    g.bench_function("small", |b| b.iter(|| Document::parse(black_box(&small))));
    g.bench_function("medium", |b| b.iter(|| Document::parse(black_box(&medium))));
    g.finish();
}

fn bench_lint(c: &mut Criterion) {
    let small = load_fixture("small.md");
    let medium = load_fixture("medium.md");
    let defaults = RuleSet::stdlib_defaults();
    let all = RuleSet::stdlib_all();

    // Lint-only: parse outside `b.iter` so we measure the rule loop.
    let small_doc = Document::parse(&small);
    let medium_doc = Document::parse(&medium);

    {
        let mut g = c.benchmark_group("lint");
        g.bench_function("defaults/small", |b| {
            b.iter(|| black_box(&small_doc).lint(black_box(&defaults)));
        });
        g.bench_function("defaults/medium", |b| {
            b.iter(|| black_box(&medium_doc).lint(black_box(&defaults)));
        });
        g.bench_function("all/small", |b| {
            b.iter(|| black_box(&small_doc).lint(black_box(&all)));
        });
        g.bench_function("all/medium", |b| {
            b.iter(|| black_box(&medium_doc).lint(black_box(&all)));
        });
        g.finish();
    }

    {
        // Full-pipeline numbers: parse + lint together for both sizes.
        let mut g = c.benchmark_group("parse_plus_lint");
        g.bench_function("defaults/small", |b| {
            b.iter(|| {
                let d = Document::parse(black_box(&small));
                black_box(d.lint(black_box(&defaults)))
            });
        });
        g.bench_function("defaults/medium", |b| {
            b.iter(|| {
                let d = Document::parse(black_box(&medium));
                black_box(d.lint(black_box(&defaults)))
            });
        });
        g.finish();
    }
}

fn bench_corpus(c: &mut Criterion) {
    let sources = load_corpus();
    let defaults = RuleSet::stdlib_defaults();
    let all = RuleSet::stdlib_all();

    let mut g = c.benchmark_group("corpus");
    // Parse-only over the full tree, rayon-parallel.
    g.bench_function("none", |b| {
        b.iter(|| {
            sources.par_iter().for_each(|src| {
                let d = Document::parse(black_box(src));
                black_box(d);
            });
        });
    });
    g.bench_function("defaults", |b| {
        b.iter(|| {
            sources.par_iter().for_each(|src| {
                let d = Document::parse(black_box(src));
                black_box(d.lint(black_box(&defaults)));
            });
        });
    });
    g.bench_function("all", |b| {
        b.iter(|| {
            sources.par_iter().for_each(|src| {
                let d = Document::parse(black_box(src));
                black_box(d.lint(black_box(&all)));
            });
        });
    });
    g.finish();
}

criterion_group!(benches, bench_parse, bench_lint, bench_corpus);
criterion_main!(benches);
