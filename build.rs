// Regenerate `docs/configuration.md` from the schema metadata table
// below. The build script is the only consumer of this data, so the
// table, the preamble, and the renderer all live here.
//
// To add or rename a TOML key: edit `SCHEMA_FIELDS`. To change the
// prose, edit `PREAMBLE` or `render_section`. Re-running `cargo build`
// rewrites the doc; the write is skipped when content is unchanged so
// the worktree doesn't go dirty on every build.

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
        key: "fmt.wrap",
        ty: "\"keep\" | \"no\" | int",
        default: "\"keep\"",
        description: "Wrap mode for prose paragraphs. `keep` leaves existing breaks alone; `no` forbids new breaks; an integer wraps at that column.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.italic",
        ty: "\"asterisk\" | \"underscore\" | \"preserve\"",
        default: "\"asterisk\"",
        description: "Italic delimiter normalisation policy.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.list-marker",
        ty: "\"dash\" | \"asterisk\" | \"plus\" | \"preserve\"",
        default: "\"dash\"",
        description: "Unordered-list bullet normalisation.",
        cli_override: None,
    },
    FieldDoc {
        key: "fmt.ordered-list",
        ty: "\"consistent\" | \"preserve\"",
        default: "\"consistent\"",
        description: "Ordered-list number normalisation. `consistent` renumbers from 1; `preserve` keeps the source numbering verbatim.",
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
        ty: "\"bare\" | \"angle\"",
        default: "\"bare\"",
        description: "Destination style for reference-link and inline-link URLs.",
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
    // ---- [fmt.frontmatter] -------------------------------------------
    FieldDoc {
        key: "fmt.frontmatter.preserve",
        ty: "bool",
        default: "true",
        description: "Whether to emit document frontmatter byte-verbatim. `false` strips it.",
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

- `lint.rules` — `--rules`
- (formatter mode is exposed via `--mode` but is not currently a
  config-file knob)
- `--no-suppress` toggles whether `<!-- mdwright: allow ... -->`
  comments are honoured; there is no config-file equivalent.

All other `[fmt]` knobs are config-file-only.

## Schema reference

"#;

fn render_configuration_md() -> String {
    let mut out = String::with_capacity(8192);
    out.push_str(PREAMBLE);
    out.push_str(
        "<!-- BEGIN GENERATED — do not edit. Regenerate by running `cargo build` after editing `build.rs`. -->\n\n",
    );
    render_section(&mut out, "[lint]", "lint.");
    render_section(&mut out, "[fmt]", "fmt.");
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
        let cli = field.cli_override.unwrap_or("—");
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

fn main() -> std::io::Result<()> {
    let body = render_configuration_md();
    let path = std::path::Path::new("docs/src/configuration.md");
    let current = std::fs::read_to_string(path).unwrap_or_default();
    if current != body {
        std::fs::write(path, body)?;
    }
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
