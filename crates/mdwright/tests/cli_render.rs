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

use std::fs;
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
    run_command("render", args, stdin, &[])
}

fn run_render_with_env(args: &[&str], stdin: &str, envs: &[(&str, &str)]) -> (bool, String, String) {
    run_command("render", args, stdin, envs)
}

fn run_preview(args: &[&str], stdin: &str) -> (bool, String, String) {
    run_command("preview", args, stdin, &[])
}

fn run_command(subcommand: &str, args: &[&str], stdin: &str, envs: &[(&str, &str)]) -> (bool, String, String) {
    let mut child = Command::new(mdwright_bin())
        .arg(subcommand)
        .args(args)
        .envs(envs.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mdwright");
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

fn contains_ansi(text: &str) -> bool {
    text.contains("\u{1b}[")
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

#[test]
fn render_captured_stdout_is_raw_html_but_color_always_highlights() {
    let (ok, stdout, stderr) = run_render(&[], "# Title\n");
    assert!(ok, "render exited non-zero; stderr:\n{stderr}");
    assert!(stdout.contains("<h1>Title</h1>"), "expected HTML:\n{stdout}");
    assert!(!contains_ansi(&stdout), "captured default render should be raw HTML");

    let (ok, stdout, stderr) = run_render(&["--color=always"], "# Title\n");
    assert!(ok, "render exited non-zero; stderr:\n{stderr}");
    assert!(
        stdout.contains("Title"),
        "highlighted HTML should preserve text:\n{stdout}"
    );
    assert!(contains_ansi(&stdout), "--color=always should ANSI-highlight HTML");
}

#[test]
fn render_open_writes_html_and_keeps_stdout_empty() {
    let (ok, stdout, stderr) = run_render_with_env(&["--open"], "# Browser\n", &[("MDWRIGHT_OPEN_DRY_RUN", "1")]);
    assert!(ok, "render --open exited non-zero; stderr:\n{stderr}");
    assert!(stdout.is_empty(), "render --open must not write HTML to stdout");
    assert!(
        stderr.contains("mdwright: opened "),
        "render --open should report the temp path:\n{stderr}"
    );
    let path = stderr
        .trim()
        .strip_prefix("mdwright: opened ")
        .expect("opened path prefix");
    let html = fs::read_to_string(path).expect("read opened html");
    assert!(
        html.contains("<h1>Browser</h1>"),
        "opened file should contain rendered HTML"
    );
    let _ignored = fs::remove_file(path);
}

#[test]
fn preview_renders_terminal_text_not_html() {
    let (ok, stdout, stderr) = run_preview(&[], "# Title\n\nSee [site](https://example.com).\n");
    assert!(ok, "preview exited non-zero; stderr:\n{stderr}");
    assert!(
        stdout.contains("Title"),
        "preview should contain heading text:\n{stdout}"
    );
    assert!(
        !stdout.contains("<h1>") && !stdout.contains("<a "),
        "preview should not emit HTML tags:\n{stdout}"
    );
}

#[test]
fn preview_math_modes_are_visible_and_fallback_is_successful() {
    let source = "Inline \\( \\alpha_i \\) and \\( \\color{red}{x} \\).\n";
    let (ok, stdout, stderr) = run_preview(&["--math=unicode"], source);
    assert!(ok, "preview unicode exited non-zero; stderr:\n{stderr}");
    assert!(
        stdout.contains("αᵢ"),
        "supported math should render to Unicode:\n{stdout}"
    );
    assert!(
        stdout.contains(r"\color{red}{x}"),
        "unsupported math should fall back to source:\n{stdout}"
    );

    let (ok, stdout, stderr) = run_preview(&["--math=source"], source);
    assert!(ok, "preview source exited non-zero; stderr:\n{stderr}");
    assert!(
        stdout.contains(r"\( \alpha_i \)"),
        "source mode should preserve math source:\n{stdout}"
    );
}

#[test]
fn preview_color_policy_is_explicit() {
    let source = "# Title\n\n```rust\nfn main() {}\n```\n";
    let (ok, stdout, stderr) = run_preview(&["--color=never"], source);
    assert!(ok, "preview color=never exited non-zero; stderr:\n{stderr}");
    assert!(!contains_ansi(&stdout), "--color=never should not emit ANSI");

    let (ok, stdout, stderr) = run_preview(&["--color=always"], source);
    assert!(ok, "preview color=always exited non-zero; stderr:\n{stderr}");
    assert!(contains_ansi(&stdout), "--color=always should emit ANSI");
}
