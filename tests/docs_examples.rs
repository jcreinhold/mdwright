#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test harness; assertions surface as panics"
)]

//! CI gate: every fenced code block in `docs/src/**/*.md` must parse
//! as the validator named by its info-string.
//!
//! - `markdown` / `md` → must parse with `pulldown-cmark` (matching
//!   the options in `src/ir.rs`).
//! - `toml`            → must parse with [`mdwright::Config::load_explicit`].
//! - `toml,no-check`   → skipped (escape hatch for non-config TOML).
//!
//! Other info-strings (`rust`, `sh`, `yaml`, …) are ignored. Files
//! under `docs/src/rules/` are skipped because their bodies are
//! auto-generated; broken examples there would surface as drift in
//! `tests/rule_docs_in_sync.rs`.

use std::path::{Path, PathBuf};

use mdwright::Config;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use walkdir::WalkDir;

fn docs_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs").join("src")
}

#[test]
fn docs_examples_validate() {
    let root = docs_root();
    let mut failures: Vec<String> = Vec::new();

    for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if is_skipped(&root, path) {
            continue;
        }

        let body = std::fs::read_to_string(path).expect("read doc");
        for block in fenced_blocks(&body) {
            if let Err(msg) = validate(&block) {
                failures.push(format!("{}: {msg}", path.strip_prefix(&root).unwrap_or(path).display()));
            }
        }
    }

    if !failures.is_empty() {
        let mut report = format!("{} broken doc example(s):\n", failures.len());
        for f in &failures {
            report.push_str("  - ");
            report.push_str(f);
            report.push('\n');
        }
        panic!("{report}");
    }
}

/// Skip auto-generated pages: drift gates already cover them.
fn is_skipped(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return true;
    };
    let head = rel.iter().next().and_then(|c| c.to_str()).unwrap_or("");
    matches!(head, "rules")
        || rel == Path::new("configuration.md")
        || rel == Path::new("reference/cli.md")
        || rel == Path::new("reference/diagnostic-schema.md")
}

#[derive(Debug)]
struct Block {
    info: String,
    body: String,
}

fn fenced_blocks(source: &str) -> Vec<Block> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);

    let mut out = Vec::new();
    let mut current: Option<(String, String)> = None;
    for ev in Parser::new_ext(source, opts) {
        match ev {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                current = Some((info.into_string(), String::new()));
            }
            Event::Text(t) => {
                if let Some((_, body)) = current.as_mut() {
                    body.push_str(&t);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((info, body)) = current.take() {
                    out.push(Block { info, body });
                }
            }
            Event::Start(_)
            | Event::End(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_) => {}
        }
    }
    out
}

fn validate(block: &Block) -> Result<(), String> {
    let info = block.info.trim();
    let mut tags = info.split(',').map(str::trim);
    let lang = tags.next().unwrap_or("");
    let no_check = tags.any(|t| t == "no-check");

    if no_check {
        return Ok(());
    }

    match lang {
        "markdown" | "md" => validate_markdown(&block.body),
        "toml" => validate_toml(&block.body),
        _ => Ok(()),
    }
}

fn validate_markdown(body: &str) -> Result<(), String> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);

    let count = Parser::new_ext(body, opts).count();
    if !body.trim().is_empty() && count == 0 {
        return Err("markdown block produced an empty event stream".to_owned());
    }
    Ok(())
}

fn validate_toml(body: &str) -> Result<(), String> {
    let tmp = tempfile::Builder::new()
        .prefix("mdwright-doc-")
        .suffix(".toml")
        .tempfile()
        .map_err(|e| format!("create temp file: {e}"))?;
    std::fs::write(tmp.path(), body).map_err(|e| format!("write temp file: {e}"))?;
    Config::load_explicit(tmp.path()).map_err(|e| format!("invalid config TOML: {e}"))?;
    Ok(())
}
