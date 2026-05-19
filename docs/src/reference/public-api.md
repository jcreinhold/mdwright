# Public API Surface

Descriptive snapshot of the public crates. The API is still pre-1.0; import paths and operation shapes may change in
minor releases under the [pre-1.0 caveats](semver.md#pre-10-caveats).

The workspace has no facade crate. The `mdwright` package is the command-line delivery crate; library users depend on
the component crate that owns the capability they need.

## Document Facts

| Item | Kind | Reached via |
| --- | --- | --- |
| `Document` | struct | `mdwright_document::Document`; parse/query handle for Markdown source |
| `ParseError` | struct | controlled failure when parser execution cannot safely recognise source |
| `ParseOptions` | struct | explicit Markdown recognition policy |
| `ExtensionOptions`, `GfmOptions`, `GfmAutolinkPolicy`, `MystOptions`, `PandocOptions` | types | fields under `ParseOptions` |
| `TextSlice`, `InlineCode`, `CodeBlock` | structs | returned by `Document` query methods |
| `HtmlBlock`, `InlineHtml`, `Heading` | structs | returned by `Document` query methods |
| `ListGroup`, `ListItem`, `LinkDef` | structs | returned by `Document` query methods |
| `Frontmatter`, `FrontmatterDelimiter` | types | returned by `Document::frontmatter` |
| `AutolinkFact`, `AutolinkOrigin` | types | returned by `Document::autolinks` |
| `Suppression`, `SuppressionKind`, `AllowScope` | types | returned by `Document::suppressions` |
| `LineIndex`, `LineIndexError` | types | byte/line/column lookup |
| `MathRegion`, `MathSpan`, `MathError` | types | math facts exposed through `Document` |
| `RenderOptions`, `RenderProfile` | types | HTML spelling policy for source-to-HTML helpers |
| `render_html`, `render_html_with_options`, `render_html_with_render_options` | fns | fallible CLI `render` and formatter verification helpers |
| `contains_rejected_control_chars` | fn | CLI input policy and fuzz harnesses |

`Document` is parse/query only. Linting, formatting, and safe-fix application are owned by their operation crates.

## Formatting

| Item | Kind | Reached via |
| --- | --- | --- |
| `FmtOptions` | struct | formatter policy |
| `FormatError` | enum | `format_validated` error |
| `format_document` | fn | format an already parsed `Document` |
| `format_source` | fn | fallibly parse with default `ParseOptions`, then format |
| `format_validated` | fn | format and verify second-pass stability; parser failures surface as `FormatError::Parse` |
| `format_range` | fn | one-shot range formatting |
| `format_range_with_checkpoints` | fn | range formatting with a cached `CheckpointTable` |
| `CheckpointTable` | struct | block-boundary cache for editor formatting |
| `semantically_equivalent`, `first_divergence` | fns | fallible formatter semantic oracles |
| `Wrap`, `ItalicStyle`, `StrongStyle` | enums | formatter style policy |
| `ListMarkerStyle`, `OrderedListStyle`, `ThematicStyle`, `TableStyle` | enums | formatter style policy |
| `LinkDefStyle`, `Placement`, `TrailingNewline`, `EndOfLine` | enums | formatter boundary/style policy |
| `MathOptions`, `MathRender`, `HeadingAttrsStyle` | types | formatter opt-in canonicalisation policy |

## Linting

| Item | Kind | Reached via |
| --- | --- | --- |
| `RuleSet` | struct | `rules.check(&doc)` / `rules.check_with(&doc, opts)` |
| `LintRule` | trait | implemented by stdlib and downstream rules |
| `LintOptions` | struct | suppression policy for `RuleSet::check_with` |
| `Diagnostic`, `Fix`, `Severity`, `Snippet` | types | lint output |
| `DuplicateRuleName` | struct | `RuleSet::add` error |
| `apply_safe_fixes` | fn | safe-fix edit application over a parsed `Document` |
| `rule_doc_url`, `docs_url`, `DOCS_URL_DEFAULT` | items | diagnostic renderer links |

The standard rule registry is under `mdwright_lint::stdlib::{defaults, all, by_name, names}`.

## Config And Delivery

| Item | Kind | Reached via |
| --- | --- | --- |
| `Config`, `ConfigError` | types | `mdwright_config`; TOML discovery and resolved options |
| `mdwright::run_with_rules` | fn | downstream custom binaries |

The LSP server lives in the `mdwright-lsp` crate. The command crate exposes only delivery helpers, so ordinary library
users do not pull in `tokio`, `tower-lsp`, `clap`, or file-walk dependencies unless they ask for them.

## Public Modules

| Module | Why it is public |
| --- | --- |
| `mdwright_lint::stdlib` | users and custom binaries can select standard lint rules. |

## Not Public Surface

- Parser internals, pulldown event ownership, source/canonical byte mapping internals, and document tree construction.
- Formatter rewrite candidates, transactional rewrite snapshots, verification signatures, and byte application logic.
- Lint suppression maps, diagnostic sorting internals, and stdlib helper functions.
- TOML raw schema structs and config discovery internals.
- CLI and LSP state machines beyond the documented entry points above.
