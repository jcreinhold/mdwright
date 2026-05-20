//! Generator for `docs/src/configuration.md`.
//!
//! The page is prose (the `PREAMBLE` constant) followed by a generated
//! schema-reference block. The block is built from the `SCHEMA_FIELDS`
//! table; to add or rename a TOML key, edit that table and re-run
//! `cargo xtask doc-config`. Drift is gated in CI by
//! `tests/config_docs_in_sync.rs`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::Drift;

/// Workspace-relative path to the rendered configuration reference.
pub const CONFIG_DOC_PATH: &str = "docs/src/configuration.md";

/// One row in the configuration reference table.
struct FieldDoc {
    /// Dotted TOML path, e.g. `"fmt.wrap"`.
    key: &'static str,
    /// Human-readable type, e.g. `"\"keep\" | \"no\" | int"`.
    ty: &'static str,
    /// Default value as it would appear in TOML, e.g. `"\"keep\""`.
    default: &'static str,
    /// Prose description, one sentence.
    description: &'static str,
    /// CLI flag that overrides this key, or `None` for file-only knobs.
    cli_override: Option<&'static str>,
}

const SCHEMA_FIELDS: &[FieldDoc] = &[
    // ---- [lint] ------------------------------------------------------
    FieldDoc {
        key: "lint.rules",
        ty: "string",
        default: "\"default\"",
        description: "Rule-selection spec. Comma-separated tokens: `all`, `default`, `<name>` (start from `{<name>}`), `+<name>` (add), `-<name>` (remove).",
        cli_override: Some("--rules"),
    },
    FieldDoc {
        key: "lint.exclude",
        ty: "array of string",
        default: "[]",
        description: "Gitignore-style patterns. Matching files are dropped from lint runs. Patterns are anchored to the directory containing the config file.",
        cli_override: None,
    },
    FieldDoc {
        key: "lint.info-strings.extra",
        ty: "array of string",
        default: "[]",
        description: "Project-specific additions to the `info-string-typo` allowlist. The stdlib's default allowlist still applies.",
        cli_override: None,
    },
    // ---- [fmt] -------------------------------------------------------
    FieldDoc {
        key: "fmt.profile",
        ty: "\"preserve\" | \"mdformat\"",
        default: "\"preserve\"",
        description: "Formatter style profile. `preserve` keeps mdwright's identity-oriented defaults; `mdformat` applies mdformat-compatible defaults where verified rewrites can preserve semantics. Explicit `[fmt]` keys override profile defaults.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.wrap",
        ty: "\"keep\" | \"no\" | int",
        default: "\"keep\"",
        description: "Wrap mode for prose paragraphs. `keep` leaves existing breaks alone; `no` forbids new breaks; an integer enforces that column for breakable lines in every formatter profile.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.wrap-strategy",
        ty: "\"stable\" | \"balanced\"",
        default: "\"stable\"",
        description: "Reflow strategy used when `fmt.wrap` is an integer. `stable` greedily fills soft-break runs and is the default; `balanced` rebalances paragraphs for more even line lengths.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.italic",
        ty: "\"asterisk\" | \"underscore\" | \"preserve\"",
        default: "\"preserve\"",
        description: "Italic delimiter canonicalisation. `preserve` (default) leaves source bytes; `asterisk` / `underscore` opt into the post-pass rewrite. See [Style knobs](format/style.md).",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.strong",
        ty: "\"asterisk\" | \"underscore\" | \"preserve\"",
        default: "\"preserve\"",
        description: "Strong-emphasis delimiter canonicalisation. Independent of `fmt.italic`: `*italic*` with `__strong__` is expressible.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.list-marker",
        ty: "\"dash\" | \"asterisk\" | \"plus\" | \"preserve\"",
        default: "\"preserve\"",
        description: "Unordered-list bullet canonicalisation. Each marker is rewritten through a marker-local fact and the family commits only after verification.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.lists.continuation-indent",
        ty: "\"marker-width\" | \"four-space\"",
        default: "\"marker-width\"",
        description: "Continuation indentation for wrapped list-item paragraphs. `marker-width` aligns to the source marker width; `four-space` matches mdformat's list continuation spelling. The mdformat profile defaults this key to `four-space`.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.ordered-list",
        ty: "\"one\" | \"consistent\" | \"preserve\"",
        default: "\"preserve\"",
        description: "Ordered-list number canonicalisation. `one` rewrites markers to `1.` only when verification preserves the list start; `consistent` renumbers each list from the source's first item; `preserve` keeps source numbering verbatim.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.thematic-break",
        ty: "\"dash\" | \"asterisk\" | \"underscore\" | \"underscore-70\" | \"preserve\"",
        default: "\"preserve\"",
        description: "Thematic-break canonicalisation. Fixed character modes preserve the source repeat count and spacing; `underscore-70` rewrites the whole break line to mdformat's 70 underscores.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.trailing-newline",
        ty: "\"preserve\" | \"strip\" | \"ensure\" | bool",
        default: "\"preserve\"",
        description: "Trailing-newline policy at the document boundary. `true` is accepted as a synonym for `ensure` and `false` for `strip` (legacy schema).",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.end-of-line",
        ty: "\"lf\" | \"crlf\" | \"keep\"",
        default: "\"lf\"",
        description: "Line-ending normalisation. `keep` adopts the first newline seen in the source.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.exclude",
        ty: "array of string",
        default: "[]",
        description: "Formatter-specific exclude globs, independent of `[lint] exclude`.",
        cli_override: None,
    },
    // ---- [fmt.refs] --------------------------------------------------
    FieldDoc {
        key: "fmt.refs.placement",
        ty: "\"end\" | \"preserve\"",
        default: "\"end\"",
        description: "Where reference-link definitions are emitted: gathered and sorted at the end of the document, or kept in source order.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.refs.style",
        ty: "\"bare\" | \"angle\" | \"preserve\"",
        default: "\"preserve\"",
        description: "Destination style for reference-link and inline-link URLs. `preserve` (default) keeps each destination's source form; `bare` strips wrapping `<…>` where the bare form would still parse; `angle` wraps every destination in `<…>`.",
        cli_override: None,
    },
    // ---- [fmt.footnotes] ---------------------------------------------
    FieldDoc {
        key: "fmt.footnotes.placement",
        ty: "\"end\" | \"preserve\"",
        default: "\"preserve\"",
        description: "Where footnote definitions are emitted. Default is `preserve` because pulldown-cmark's HTML renderer ties footnote position to parse order; moving definitions would change the rendered HTML.",
        cli_override: None,
    },
    // ---- [fmt.tables] -----------------------------------------------
    FieldDoc {
        key: "fmt.tables.style",
        ty: "\"preserve\" | \"pad\"",
        default: "\"preserve\"",
        description: "GFM table spacing policy. `preserve` keeps source cell spacing; `pad` aligns cells and delimiter rows to mdformat-compatible widths when verification preserves semantics.",
        cli_override: None,
    },
    // ---- [fmt.frontmatter] -------------------------------------------
    FieldDoc {
        key: "fmt.frontmatter.preserve",
        ty: "bool",
        default: "true",
        description: "Whether to emit document frontmatter byte-verbatim. `false` strips it.",
        cli_override: None,
    },
    // ---- [fmt] heading attribute trailers ----------------------------
    FieldDoc {
        key: "fmt.heading-attrs",
        ty: "\"preserve\" | \"canonicalise\"",
        default: "\"preserve\"",
        description: "ATX heading `{#id .class key=val}` trailer emission. `preserve` (default) emits the source trailer byte-verbatim. `canonicalise` emits id first, then classes (source order), then key=value pairs (source order). See [Markdown extensions](concepts/extensions.md#heading-attribute-lists).",
        cli_override: None,
    },
    // ---- [parse.extensions] -----------------------------------------
    FieldDoc {
        key: "parse.extensions.gfm.autolinks",
        ty: "\"disabled\" | \"urls\" | \"urls-and-emails\"",
        default: "\"urls-and-emails\"",
        description: "Recognise GFM bare URL and email autolinks as document facts and render them as links. Use `urls` to leave bare emails as text or `disabled` for strict CommonMark-style text treatment.",
        cli_override: None,
    },
    FieldDoc {
        key: "parse.extensions.gfm.tagfilter",
        ty: "bool",
        default: "true",
        description: "Apply GFM tagfiltering when rendering or building semantic signatures. This escapes the raw HTML tags that cmark-gfm filters, without rewriting source bytes.",
        cli_override: None,
    },
    // ---- [render] ----------------------------------------------------
    FieldDoc {
        key: "render.profile",
        ty: "\"pulldown\" | \"cmark-gfm\"",
        default: "\"pulldown\"",
        description: "HTML spelling profile for `mdwright render`. `pulldown` preserves the default renderer; `cmark-gfm` matches cmark-gfm's quote, link-destination, table, task-list, and HTML-block spelling where parser semantics already agree.",
        cli_override: Some("--render-profile"),
    },
    FieldDoc {
        key: "parse.extensions.definition-lists",
        ty: "bool",
        default: "true",
        description: "Recognise `Term\\n: definition\\n` definition lists. Default on; turn off on non-mkdocs corpora to suppress recognition.",
        cli_override: None,
    },
    FieldDoc {
        key: "parse.extensions.abbreviation-lists",
        ty: "bool",
        default: "true",
        description: "Recognise `*[ABBR]: definition` abbreviation declarations as a scan-and-preserve overlay. mdwright does not expand occurrences; the downstream renderer does.",
        cli_override: None,
    },
    FieldDoc {
        key: "parse.extensions.heading-attribute-lists",
        ty: "bool",
        default: "true",
        description: "Recognise `# Heading {#id .class}` trailers via pulldown's `ENABLE_HEADING_ATTRIBUTES`. When off, the trailer reads as plain text in the heading body.",
        cli_override: None,
    },
    FieldDoc {
        key: "parse.extensions.block-attribute-lists",
        ty: "bool",
        default: "true",
        description: "Recognise `{ .class }` on a line by itself after a non-empty block as a scan-and-preserve overlay. Inline attribute lists (mid-paragraph) are out of scope.",
        cli_override: None,
    },
    // ---- [parse.extensions.myst] ------------------------------------
    FieldDoc {
        key: "parse.extensions.myst.directive-containers",
        ty: "bool",
        default: "true",
        description: "Recognise MyST `:::{name}` directive containers (with `:KEY: value` options) as a scan-and-preserve overlay. mdwright does not expand directives; downstream renderers (Sphinx, jupyter-book) do.",
        cli_override: None,
    },
    FieldDoc {
        key: "parse.extensions.myst.inline-roles",
        ty: "bool",
        default: "true",
        description: "Recognise MyST `` {role}`payload` `` inline roles as a scan-and-preserve overlay inside paragraph text.",
        cli_override: None,
    },
    FieldDoc {
        key: "parse.extensions.myst.substitution-references",
        ty: "bool",
        default: "true",
        description: "Recognise MyST `{{name}}` inline substitution references as a scan-and-preserve overlay. Declarations live in YAML frontmatter under `myst_substitutions:` and round-trip through the frontmatter verbatim path.",
        cli_override: None,
    },
    FieldDoc {
        key: "parse.extensions.myst.comments",
        ty: "bool",
        default: "true",
        description: "Recognise MyST `%` line comments at line-start as a scan-and-preserve overlay.",
        cli_override: None,
    },
    // ---- [parse.extensions.pandoc] ----------------------------------
    FieldDoc {
        key: "parse.extensions.pandoc.fenced-divs",
        ty: "bool",
        default: "true",
        description: "Recognise Pandoc `::: {.cls}` fenced div openers (attribute form). Closer is a colon-only line of matching count.",
        cli_override: None,
    },
    FieldDoc {
        key: "parse.extensions.pandoc.short-form-divs",
        ty: "bool",
        default: "true",
        description: "Recognise Pandoc `:::name` fenced div openers (short form).",
        cli_override: None,
    },
    FieldDoc {
        key: "parse.extensions.pandoc.inline-attribute-spans",
        ty: "bool",
        default: "true",
        description: "Recognise Pandoc `[content]{.cls}` inline attribute spans as a scan-and-preserve overlay.",
        cli_override: None,
    },
];

const PREAMBLE: &str = r#"# Configuration

mdwright reads configuration from (in precedence order):

1. The file given via `--config PATH`.
2. The nearest ancestor config discovered by walking upward from the
   current directory. At each ancestor, candidates are tried in this
   order: `.mdwright.toml`, `mdwright.toml`,
   `pyproject.toml` containing a `[tool.mdwright]` table. The walk
   stops at the filesystem root or at the first directory containing
   `.git/` (the workspace boundary).
3. Built-in defaults.

A `pyproject.toml` *without* `[tool.mdwright]` does not stop the walk;
discovery continues to the parent directory. A `.mdwright.toml` wins
over a `pyproject.toml` in the same directory (matching ruff's
"more-specific-name first" rule).

## Single-file integration via `pyproject.toml`

For projects that already use `pyproject.toml`, the entire mdwright
configuration can live there under `[tool.mdwright]`:

```toml
# pyproject.toml
[tool.mdwright]
lint.rules = "default,+latex-command"

[tool.mdwright.fmt]
wrap = 100
```

## CLI overrides

The following knobs accept CLI flags that take precedence over the
config file:

- `lint.rules`: `--rules`
- `render.profile`: `mdwright render --render-profile`
- `--no-suppress` toggles whether `<!-- mdwright: allow ... -->`
  comments are honoured; there is no config-file equivalent.

All `[fmt]` knobs are config-file-only.

## Schema reference

"#;

/// Build the expected contents of [`CONFIG_DOC_PATH`].
#[must_use]
pub fn render() -> String {
    let mut out = String::with_capacity(8192);
    out.push_str(PREAMBLE);
    out.push_str("<!-- BEGIN GENERATED: do not edit. Regenerate by running `cargo xtask doc-config`. -->\n\n");
    render_section(&mut out, "[lint]", "lint.");
    render_section(&mut out, "[fmt]", "fmt.");
    render_section(&mut out, "[parse]", "parse.");
    render_section(&mut out, "[render]", "render.");
    out.push_str("<!-- END GENERATED -->\n");
    out
}

fn render_section(out: &mut String, heading: &str, prefix: &str) {
    out.push_str("### `");
    out.push_str(heading);
    out.push_str("` and nested tables\n\n");
    out.push_str("| Key | Type | Default | CLI override | Description |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for field in SCHEMA_FIELDS {
        if !field.key.starts_with(prefix) {
            continue;
        }
        let cli = field.cli_override.unwrap_or("none");
        // Escape pipe characters in the type column so the table parses.
        let ty_escaped = field.ty.replace('|', "\\|");
        out.push_str("| `");
        out.push_str(field.key);
        out.push_str("` | ");
        out.push_str(&ty_escaped);
        out.push_str(" | `");
        out.push_str(field.default);
        out.push_str("` | `");
        out.push_str(cli);
        out.push_str("` | ");
        out.push_str(field.description);
        out.push_str(" |\n");
    }
    out.push('\n');
}

/// Write the rendered page to disk.
///
/// # Errors
///
/// Surfaces I/O failures from creating the parent directory or
/// writing the file.
pub fn regenerate(workspace: &Path) -> Result<()> {
    let body = render();
    let path: PathBuf = workspace.join(CONFIG_DOC_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Compare the rendered page to its on-disk counterpart. Returns a
/// vector of [`Drift`] entries; empty means no drift.
///
/// # Errors
///
/// Surfaces I/O failures other than `NotFound`; a missing file counts
/// as drift, not an error.
pub fn check(workspace: &Path) -> Result<Vec<Drift>> {
    let expected = render();
    let path: PathBuf = workspace.join(CONFIG_DOC_PATH);
    let actual = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    if actual != expected {
        Ok(vec![Drift { path, expected }])
    } else {
        Ok(Vec::new())
    }
}
