#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test harness; assertions surface as panics"
)]

//! End-to-end smoke test for `mdwright render`.
//!
//! The subcommand pipes the formatted output through the same HTML
//! renderer the `format_validated` gate uses. This test invokes the
//! locally-built binary against a math-bearing fixture from stdin
//! and asserts the rendered HTML contains the expected math regions
//! under each `--math-render` mode.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn mdwright_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mdwright"))
}

const MATH_DOC: &str = r"# Math

Inline: \( A \). Display:

\[ A = B \]
";

fn run_render(args: &[&str], stdin: &str) -> (bool, String, String) {
    let mut child = Command::new(mdwright_bin())
        .arg("render")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mdwright render");
    child
        .stdin
        .as_mut()
        .expect("stdin handle")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait render");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn render_default_passes_math_through_verbatim() {
    let (ok, stdout, stderr) = run_render(&[], MATH_DOC);
    assert!(ok, "render exited non-zero; stderr:\n{stderr}");
    // The HTML renderer (`render_html`) does not enable pulldown's
    // math extension, so `\(`/`\[` are treated as escape sequences
    // and the backslashes drop out of the rendered HTML. Default
    // (`none`) mode does no rewriting; the source bytes — including
    // the inline single-line shape of `\[ A = B \]` — are preserved
    // by the identity structural emit, so the HTML carries the same
    // shape modulo the dropped backslashes.
    assert!(
        stdout.contains("( A )") && stdout.contains("[ A = B ]"),
        "expected verbatim math in HTML; got:\n{stdout}"
    );
    // The Dollar rewrite must not have happened.
    assert!(
        !stdout.contains("$A$") && !stdout.contains("$$"),
        "default mode unexpectedly rewrote math:\n{stdout}"
    );
}

#[test]
fn render_dollar_rewrites_math_in_html() {
    let (ok, stdout, stderr) = run_render(&["--math-render=dollar"], MATH_DOC);
    assert!(ok, "render exited non-zero; stderr:\n{stderr}");
    assert!(
        stdout.contains("$A$") && stdout.contains("$$ A = B $$"),
        "expected dollar-rewritten math in HTML; got:\n{stdout}"
    );
    // The original bracket / paren forms must be gone.
    assert!(
        !stdout.contains(r"\(") && !stdout.contains(r"\["),
        "bracket/paren delimiters leaked through:\n{stdout}"
    );
}

#[test]
fn render_profile_override_uses_cmark_gfm_html_spelling() {
    let source = "| foo | bar |\n| --- | --- |\n| baz | bim |\n";
    let (ok, stdout, stderr) = run_render(&["--render-profile=cmark-gfm"], source);
    assert!(ok, "render exited non-zero; stderr:\n{stderr}");
    assert!(
        stdout.contains("<thead>\n<tr>\n<th>foo</th>\n<th>bar</th>\n</tr>\n</thead>"),
        "expected cmark-gfm table spelling; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("<table><thead>"),
        "pulldown compact table spelling leaked through:\n{stdout}"
    );
}
