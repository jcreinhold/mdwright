# Public API surface

Descriptive snapshot of the `mdwright` library crate's public surface at
the current release. **Pre-1.0**, this surface may evolve in minor
versions per the [Pre-1.0 caveats](semver.md#pre-10-caveats) in the
semver policy; items listed here are exported today, but their continued
existence and signature are not yet a stability promise.

The audit rule is simple: every item in [`src/lib.rs`'s `pub use`
block](https://github.com/jcreinhold/mdwright/blob/main/src/lib.rs) is
reachable through code that lives outside its defining module — either
called directly, returned by a public method, present as a field of a
public struct, or carried as the error type of a public `Result`. Items
that are not reachable from outside `src/<module>/` are `pub(crate)` and
do not appear here.

The "Reached via" column names *one* concrete caller or reachability
path. Many items have more; this column exists to prove the item is on
the public surface for a reason, not to enumerate every use.

## Config (`mdwright::config`)

| Item | Kind | Reached via |
| --- | --- | --- |
| `Config` | struct | `src/cli.rs` (loads via `Config::discover`) |
| `ConfigError` | struct | error type of `Config::load_explicit` / `Config::discover` |
| `FmtOptions` | struct | `src/cli.rs`, `src/lsp.rs`, `tests/properties.rs` |
| `ExtensionOptions` | struct | returned by `FmtOptions::extensions` |
| `MystOptions` | struct | `pub myst: MystOptions` field of `ExtensionOptions` |
| `PandocOptions` | struct | `pub pandoc: PandocOptions` field of `ExtensionOptions` |
| `MathOptions` | struct | returned by `FmtOptions::math`; `tests/golden_math.rs` |
| `MathRender` | enum | accepted by `FmtOptions::with_math_render`; `src/cli.rs` |
| `HeadingAttrsStyle` | enum | accepted by `FmtOptions::with_heading_attrs`; `tests/regressions_heading_attrs.rs` |
| `FormatMode` | enum | accepted by `FmtOptions::with_mode`; `src/cli.rs` |
| `Wrap` | enum | returned by `FmtOptions::wrap` |
| `ItalicStyle` | enum | returned by `FmtOptions::italic` |
| `StrongStyle` | enum | returned by `FmtOptions::strong` |
| `LinkDefStyle` | enum | returned by `FmtOptions::link_def_style` |
| `ListMarkerStyle` | enum | returned by `FmtOptions::list_marker` |
| `OrderedListStyle` | enum | returned by `FmtOptions::ordered_list` |
| `ThematicStyle` | enum | returned by `FmtOptions::thematic_break_style` |
| `Placement` | enum | returned by `FmtOptions::link_def_placement` / `footnote_placement` |
| `EndOfLine` | enum | returned by `FmtOptions::end_of_line` |
| `TrailingNewline` | enum | returned by `FmtOptions::trailing_newline` |

## Diagnostic (`mdwright::diagnostic`)

| Item | Kind | Reached via |
| --- | --- | --- |
| `Diagnostic` | struct | `src/cli.rs`, `examples/extending/src/no_todo.rs`, extensive tests |
| `Fix` | struct | `pub fix: Option<Fix>` field of `Diagnostic` |
| `Severity` | enum | `pub severity: Severity` field of `Diagnostic`; `src/cli.rs` pretty-printer |
| `Snippet` | struct | constructed by external diagnostic renderers; `src/cli.rs:1208` |
| `rule_doc_url` | fn | `src/cli.rs`, `src/lsp.rs` |
| `docs_url` | fn | base for `rule_doc_url`; honours `MDWRIGHT_DOCS_URL` for downstream renderers |
| `DOCS_URL_DEFAULT` | const | fallback constant inspectable by downstream renderers that bypass `docs_url` |

## Discover (`mdwright::discover`)

| Item | Kind | Reached via |
| --- | --- | --- |
| `discover_markdown` | fn | `src/cli.rs`; `tests/discover_symlink_loop.rs` |

## Document (`mdwright::document`)

| Item | Kind | Reached via |
| --- | --- | --- |
| `Document` | struct | `src/cli.rs`, `src/lsp.rs`, `examples/extending/src/no_todo.rs` |
| `LintOptions` | struct | accepted by `Document::lint_with`; `tests/suppression.rs` |
| `FormatError` | enum | error type of `Document::format_validated`; `tests/regressions_heading_attrs.rs` |
| `render_html` | fn | `src/cli.rs` (the `mdwright render` subcommand) |

## IR (`mdwright::ir`)

Every IR type listed here is returned by a public query method on
[`Document`](#document-mdwrightdocument), or is a field of a type that is.
Plugin rule authors read these types; they should not construct them
directly.

| Item | Kind | Reached via |
| --- | --- | --- |
| `TextSlice` | struct | returned by `Document::prose_chunks` |
| `InlineCode` | struct | returned by `Document::inline_codes` |
| `CodeBlock` | struct | returned by `Document::code_blocks` |
| `HtmlBlock` | struct | returned by `Document::html_blocks` |
| `InlineHtml` | struct | returned by `Document::inline_html` |
| `Heading` | struct | returned by `Document::headings` |
| `ListGroup` | struct | returned by `Document::list_groups` |
| `ListItem` | struct | `pub items: Vec<ListItem>` field of `ListGroup` |
| `LinkDef` | struct | returned by `Document::link_defs` |
| `Frontmatter` | struct | returned by `Document::frontmatter` |
| `FrontmatterDelimiter` | enum | `pub delimiter: FrontmatterDelimiter` field of `Frontmatter` |
| `Suppression` | struct | returned by `Document::suppressions` |
| `SuppressionKind` | enum | `pub kind: SuppressionKind` field of `Suppression` |
| `AllowScope` | enum | variant data of `SuppressionKind::Allow { scope }` |

## Line index (`mdwright::line_index`)

| Item | Kind | Reached via |
| --- | --- | --- |
| `LineIndex` | struct | returned by `Document::line_index`; `src/cli.rs`, `src/lsp.rs` |

## Rules (`mdwright::rule`, `mdwright::rule_set`)

| Item | Kind | Reached via |
| --- | --- | --- |
| `LintRule` | trait | implemented by every stdlib rule and by `examples/extending/src/no_todo.rs` |
| `RuleSet` | struct | `src/cli.rs`, `src/lsp.rs`, `examples/extending/src/main.rs` |
| `DuplicateRuleName` | struct | error type of `RuleSet::add` |

## Format helpers (`mdwright::format::semantic`)

| Item | Kind | Reached via |
| --- | --- | --- |
| `semantically_equivalent` | fn | `tests/properties.rs`, `tests/gfm_spec.rs` |

## Incremental (`mdwright::incremental`)

| Item | Kind | Reached via |
| --- | --- | --- |
| `CheckpointTable` | struct | `src/cli.rs`, `src/lsp.rs` |

## Free functions (`mdwright`)

| Item | Kind | Reached via |
| --- | --- | --- |
| `format_range` | fn | `tests/properties.rs`, `benches/incremental.rs`, the lib's own doctest |
| `format_range_with_checkpoints` | fn | `src/cli.rs`, `src/lsp.rs` |
| `contains_rejected_control_chars` | fn | `src/cli.rs` (the `--reject-control-chars` flag) |

## Public modules

| Module | Why it's `pub` |
| --- | --- |
| `mdwright::cli` | entry points for downstream binaries built on top of `mdwright` (notably the `cli::run_with_rules` plugin model in `examples/extending`). |
| `mdwright::lsp` | entry point for embedding the LSP server. |
| `mdwright::stdlib` | the curated standard library of lint rules, used by `RuleSet::stdlib_defaults` / `stdlib_all` and by `examples/extending` to mix-and-match. |

## What is *not* on the public surface

- Everything in `cm/`, `format/` (other than `format::semantic`),
  `parse`, `source`, `suppression`, `tree`, and `util` is
  `pub(crate)` or private. These modules implement the formatter and
  recogniser pipelines; downstream code should not depend on their
  shape.
- The `Ir` struct (the parsed document's internal representation) is
  `pub(crate)`. Plugin rules reach individual IR slices through the
  `Document` query methods listed above.
- `tracing` log lines, the on-disk layout of `target/`, and the prose
  output of `mdwright explain` are not part of the public surface (see
  [semver.md "Not covered"](semver.md#not-covered)).
