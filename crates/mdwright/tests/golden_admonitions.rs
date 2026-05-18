#![allow(
    clippy::panic,
    reason = "test fixture loader; missing-file panics are the desired failure mode"
)]

//! mkdocs-admonition golden tests.
//!
//! mdwright emits admonition regions byte-verbatim from source. The
//! fixtures verify round-trip on every form mkdocs accepts: bare,
//! titled, multi-paragraph, collapsible (`???` / `???+`), and the
//! blank-line-between-header-and-body shape that pulldown-cmark
//! parses as two siblings instead of one paragraph.

use std::fs;
use std::path::Path;

use mdwright_document::Document;
use mdwright_format::FmtOptions;

#[test]
fn golden_admonitions() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden_admonitions");
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
        let doc = Document::parse(&input);
        let got = mdwright_format::format_document(&doc, &FmtOptions::default());
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
