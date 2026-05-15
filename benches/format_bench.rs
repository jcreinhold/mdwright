//! Criterion benches for `mdwright` formatter.
//!
//! Mirrors `lint_bench.rs`'s small / medium / corpus loadout and
//! adds:
//!
//! * `format/{small,medium}` — parse outside `b.iter`, time only
//!   format (the steady-state metric the optimization plan targets).
//! * `parse_plus_format/{small,medium}` — full pipeline per call,
//!   matches the CLI fmt-check workload.
//! * `format/wrap/{keep,80,100,120}` — wrap-mode sweep on the
//!   medium fixture; Knuth-Plass DP only runs in `At(n)` modes.
//! * `format/corpus/{none-wrap,wrap-100}` — full Kan docs tree,
//!   rayon-parallel; the headline metric for the ≥10× target.
//!
//! Numbers from these benches are machine-local; do not commit them.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};
use mdwright::{Document, FmtOptions, Wrap};
use rayon::prelude::*;
use std::hint::black_box;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Resolve the root of the documentation corpus referenced by `corpus.list`.
///
/// Order: `MDWRIGHT_CORPUS_ROOT` env var, then a sibling `kan` checkout
/// next to this crate (the common local layout: `~/Code/mdwright` next to
/// `~/Code/kan`). Panics with a setup hint if neither is found.
fn corpus_root() -> PathBuf {
    if let Some(v) = std::env::var_os("MDWRIGHT_CORPUS_ROOT") {
        return PathBuf::from(v);
    }
    if let Some(parent) = manifest_dir().parent() {
        let sibling = parent.join("kan");
        if sibling.join("docs").join("books").is_dir() {
            return sibling;
        }
    }
    #[allow(clippy::panic)]
    {
        panic!(
            "cannot locate corpus root; set MDWRIGHT_CORPUS_ROOT to a directory \
             containing the corpus paths listed in benches/corpus.list",
        )
    }
}

fn read_to_string(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

fn load_fixture(name: &str) -> String {
    let path = manifest_dir().join("benches").join("fixtures").join(name);
    read_to_string(&path).unwrap_or_else(|e| {
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
    let root = corpus_root();
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

fn bench_format(c: &mut Criterion) {
    let small = load_fixture("small.md");
    let medium = load_fixture("medium.md");
    let opts = FmtOptions::default();

    let small_doc = Document::parse(&small);
    let medium_doc = Document::parse(&medium);

    {
        let mut g = c.benchmark_group("format");
        g.bench_function("small", |b| {
            b.iter(|| black_box(&small_doc).format(black_box(&opts)));
        });
        g.bench_function("medium", |b| {
            b.iter(|| black_box(&medium_doc).format(black_box(&opts)));
        });
        g.finish();
    }
    {
        let mut g = c.benchmark_group("parse_plus_format");
        g.bench_function("small", |b| {
            b.iter(|| {
                let d = Document::parse(black_box(&small));
                black_box(d.format(black_box(&opts)))
            });
        });
        g.bench_function("medium", |b| {
            b.iter(|| {
                let d = Document::parse(black_box(&medium));
                black_box(d.format(black_box(&opts)))
            });
        });
        g.finish();
    }
}

fn bench_format_wrap(c: &mut Criterion) {
    let medium = load_fixture("medium.md");
    let medium_doc = Document::parse(&medium);

    let mut g = c.benchmark_group("format/wrap");
    for (label, wrap) in [
        ("keep", Wrap::Keep),
        ("at-80", Wrap::At(80)),
        ("at-100", Wrap::At(100)),
        ("at-120", Wrap::At(120)),
    ] {
        let opts = FmtOptions::default().with_wrap(wrap);
        g.bench_function(label, |b| {
            b.iter(|| black_box(&medium_doc).format(black_box(&opts)));
        });
    }
    g.finish();
}

fn bench_format_corpus(c: &mut Criterion) {
    let sources = load_corpus();
    let opts_default = FmtOptions::default();
    let opts_wrap100 = FmtOptions::default().with_wrap(Wrap::At(100));

    let mut g = c.benchmark_group("format/corpus");
    // Headline metric: parse + format per file, rayon-parallel, no
    // wrap (matches `fmt --check` on existing well-wrapped corpus).
    g.bench_function("none-wrap", |b| {
        b.iter(|| {
            sources.par_iter().for_each(|src| {
                let d = Document::parse(black_box(src));
                black_box(d.format(black_box(&opts_default)));
            });
        });
    });
    // Wrap variant: forces Knuth-Plass DP through every paragraph.
    g.bench_function("wrap-100", |b| {
        b.iter(|| {
            sources.par_iter().for_each(|src| {
                let d = Document::parse(black_box(src));
                black_box(d.format(black_box(&opts_wrap100)));
            });
        });
    });
    g.finish();
}

criterion_group!(
    benches,
    bench_format,
    bench_format_wrap,
    bench_format_corpus
);
criterion_main!(benches);
