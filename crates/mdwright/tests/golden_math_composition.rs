//! Math-composition golden tests.
//!
//! Mirrors `golden_math.rs` / `golden_wrap.rs`. Each fixture under
//! `tests/golden_math_composition/` is an `*.in` / `*.out` pair, with
//! an optional `*.config.toml` overriding `[fmt]` (e.g. `wrap`,
//! `math.normalise`). These fixtures exercise inline math interleaved
//! with prose, container-cascading display math, math inside
//! emphasis / links / headings, etc. — the cases that the deleted
//! block-level math overlay punted on.

#![allow(
    clippy::panic,
    reason = "test fixture loader; missing-file panics are the desired failure mode"
)]

use std::fs;
use std::path::Path;

use mdwright_config::Config;
use mdwright_document::Document;
use mdwright_format::FmtOptions;

#[test]
fn golden_math_composition() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden_math_composition");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::path);

    let mut failures: Vec<String> = Vec::new();
    let mut count = 0usize;
    for entry in &entries {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".in") else {
            continue;
        };
        let expected_path = dir.join(format!("{stem}.out"));
        let input = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let expected =
            fs::read_to_string(&expected_path).unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display()));
        let config_path = dir.join(format!("{stem}.config.toml"));
        let opts = if config_path.is_file() {
            let cfg =
                Config::load_explicit(&config_path).unwrap_or_else(|e| panic!("load {}: {e}", config_path.display()));
            cfg.fmt_options().clone()
        } else {
            FmtOptions::default()
        };
        let doc = Document::parse(&input);
        let got = mdwright_format::format_document(&doc, &opts);
        count = count.saturating_add(1);
        if got != expected {
            failures.push(format!(
                "--- {stem} ---\n--- input ---\n{input}--- expected ---\n{expected}--- got ---\n{got}--- end ---\n"
            ));
        }
    }
    assert!(count > 0, "no golden fixtures found under {}", dir.display());
    assert!(
        failures.is_empty(),
        "{}/{} golden fixture(s) failed:\n{}",
        failures.len(),
        count,
        failures.join("\n")
    );
}
