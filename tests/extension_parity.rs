//! Byte-equality parity check between mdwright and mdformat-mkdocs.
//!
//! Each `tests/regressions/extension_*.in` fixture has a paired
//! `tests/fixtures/mdformat_mkdocs/<name>.out` file generated locally
//! with `mdformat` (which loads the `mdformat-mkdocs` plugin
//! automatically when installed). The test formats the `.in` file
//! through mdwright under default options and asserts byte equality
//! against the committed `.out`. Divergences either get a fix here or
//! a documented row in `docs/src/deviations.md`; the parity test never
//! silently drifts.
//!
//! The `.out` files are committed static — CI does not run
//! mdformat-mkdocs. Regenerate them locally with:
//!
//! ```bash
//! for f in tests/regressions/extension_*.in; do
//!     name=$(basename "$f" .in)
//!     mdformat - < "$f" > "tests/fixtures/mdformat_mkdocs/${name}.out"
//! done
//! ```

#![allow(clippy::panic)]

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use mdwright::{Document, FmtOptions};

const FIXTURES: &[&str] = &[
    "extension_definition_list",
    "extension_definition_list_nested",
    "extension_abbreviation_list",
    "extension_heading_attrs",
    "extension_block_attrs",
];

fn read(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn mdwright_output_matches_mdformat_mkdocs_byte_for_byte() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let opts = FmtOptions::default();
    let mut failures: Vec<(String, String, String)> = Vec::new();
    for name in FIXTURES {
        let in_path = manifest.join("tests").join("regressions").join(format!("{name}.in"));
        let out_path = manifest
            .join("tests")
            .join("fixtures")
            .join("mdformat_mkdocs")
            .join(format!("{name}.out"));
        let src = read(&in_path);
        let expected = read(&out_path);
        let actual = Document::parse(&src).format(&opts);
        if actual != expected {
            failures.push(((*name).to_owned(), expected, actual));
        }
    }
    if !failures.is_empty() {
        let mut msg = String::from("parity divergences vs mdformat-mkdocs:\n");
        for (name, expected, actual) in &failures {
            let _ = write!(
                msg,
                "--- {name} ---\n=== expected (mdformat-mkdocs) ===\n{expected}=== actual (mdwright) ===\n{actual}\n",
            );
        }
        panic!("{msg}");
    }
}
