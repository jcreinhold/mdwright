//! Idempotence-on-mode check against vendored external corpora.
//!
//! Each file under `tests/external/<project>/*.md` (excluding
//! `SOURCES.md` attribution files) is loaded, formatted through
//! mdwright with [`FmtOptions::default`] twice, and the two outputs
//! are asserted byte-equal: `format(format(src)) == format(src)`.
//!
//! This is **idempotence-on-mode**, not source round-trip: mdwright is
//! allowed to canonicalise (rewrap paragraphs to 120 columns, normalise
//! list markers, etc.) — the round-trip bar is just that the
//! canonicalised form is a fixed point. Recognition divergences
//! (e.g. a `MyST` directive that's parsed as a definition list because
//! the recogniser missed it) surface as a non-idempotent run, because
//! the second pass reparses the formatter's bytes and sees the dropped
//! directive shape.
//!
//! The `tests/external/jupyter_book_minimal/` fixtures are vendored
//! from `jupyter-book/mystmd@main` (MIT licensed) — see
//! `tests/external/jupyter_book_minimal/SOURCES.md`.

#![allow(clippy::panic)]

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use mdwright::{Document, FmtOptions};

#[test]
fn jupyter_book_minimal_round_trips() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("external")
        .join("jupyter_book_minimal");
    let opts = FmtOptions::default();
    let mut failures: Vec<(PathBuf, String, String)> = Vec::new();
    let entries = fs::read_dir(&root).unwrap_or_else(|e| panic!("read_dir {}: {e}", root.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("dir entry: {e}"));
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) != Some("md") {
            continue;
        }
        if path.file_name().and_then(OsStr::to_str) == Some("SOURCES.md") {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let once = mdwright::format_document(&Document::parse(&src), &opts);
        let twice = mdwright::format_document(&Document::parse(&once), &opts);
        if once != twice {
            failures.push((path, once, twice));
        }
    }
    if !failures.is_empty() {
        let mut msg = String::from("external corpora idempotence failures (format twice diverges):\n");
        for (path, once, twice) in &failures {
            let _ = write!(
                msg,
                "--- {} ---\n=== once ===\n{once}=== twice ===\n{twice}\n",
                path.display(),
            );
        }
        panic!("{msg}");
    }
}
