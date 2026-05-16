#![allow(
    clippy::panic,
    reason = "test fixture loader; missing-file panics are the desired failure mode"
)]

//! Math pretty-printer golden tests.
//!
//! Each fixture is an `*.in` / `*.out` pair under `tests/golden_math/`.
//! Math normalisation is gated behind `FmtOptions::math().normalise`
//! (default `false`) — see `src/config.rs` for the reason; this
//! runner flips it on so the fixtures exercise the pretty path.

use std::fs;
use std::path::Path;

use mdwright::{Document, FmtOptions, MathOptions};

#[test]
fn golden_math() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden_math");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::path);

    let opts = FmtOptions::default().with_math(MathOptions { normalise: true });

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
        let input =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display()));
        let doc = Document::parse(&input);
        let got = doc.format(&opts);
        count = count.saturating_add(1);
        if got != expected {
            failures.push(format!(
                "--- {stem} ---\n--- input ---\n{input}--- expected ---\n{expected}--- got ---\n{got}--- end ---\n"
            ));
        }
    }
    assert!(
        count > 0,
        "no golden fixtures found under {}",
        dir.display()
    );
    assert!(
        failures.is_empty(),
        "{}/{} golden fixture(s) failed:\n{}",
        failures.len(),
        count,
        failures.join("\n")
    );
}
