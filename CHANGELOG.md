# Changelog

All notable changes to mdwright are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed (breaking, pre-1.0)

- Split the codebase into a virtual workspace of deep crates. There is no root facade package: command users install the
  `mdwright` package, while Rust library users depend directly on `mdwright-document`, `mdwright-format`,
  `mdwright-lint`, `mdwright-config`, `mdwright-lsp`, or `mdwright-math`.
- Moved the executable target into the `mdwright` package under `crates/mdwright`. The binary name remains `mdwright`,
  and CLI behaviour is unchanged.
- Moved operation methods off `Document`. `Document` is now parse/query only: use
  `mdwright_format::{format_document, format_source, format_validated}` for formatting; use `rules.check(&doc)` /
  `rules.check_with(&doc, opts)` for linting; use `mdwright_lint::apply_safe_fixes(&doc, &diags)` for safe lint fixes.
- Moved recognition toggles into `ParseOptions`. `FmtOptions` now owns formatting policy only; extension, MyST, and
  Pandoc recognition policy belong to document parsing and config resolution. Config files now use `[parse.extensions]`
  instead of the previous formatter-owned extension table.
- Formatter entry points over `Document` now honour the document's parse policy throughout rewrite snapshots,
  verification reparses, semantic signatures, and range-format checkpointing.
- `Document::parse`, `Document::parse_with_options`, `render_html`, `format_source`, and the formatter semantic oracles
  are now fallible. `mdwright-document` contains upstream parser panics and reports them as `ParseError`; CLI and LSP
  delivery surfaces report controlled parse errors instead of crashing.
- Removed the old public module-shaped facade for parser, formatter, linter, config, and LSP internals. Component crates
  expose their own narrow APIs, and the `mdwright` package exposes only command-extension helpers such as
  `run_with_rules`.
- Narrowed the pre-1.0 public surface before release. `mdwright-lsp::build_service_for_tests`, document source-map
  internals (`Source`, `CanonicalSource`, `OffsetMap`, `ByteSpan`, `OriginalSpan`), parser tree internals (`Tree`,
  `Node`, `NodeId`, `NodeKind`), `NormalisedLabel`, `find_attr_trailer_range`, and public top-level checkpoint helpers
  are no longer exported. Use `Document` fact accessors and `mdwright_format::CheckpointTable` instead.

### Architecture

- `mdwright-document` owns source coordinates, canonical source mapping, pulldown invocation, parse options, document
  facts, reference/frontmatter/list/code/html inventories, and document queries. Formatter and lint crates consume
  domain facts rather than pulldown parser events.
- `mdwright-math` owns pure TeX/math span recognition, renderer conversion helpers, and body normalisation helpers.
- `mdwright-format` owns `FmtOptions`, range formatting, semantic formatter oracles, and the private rewrite-family
  pipeline.
- `mdwright-lint` owns diagnostics, lint rules, suppression, standard-rule registry construction, and safe-fix
  application.
- `mdwright-config`, `mdwright`, and `mdwright-lsp` own configuration interpretation and delivery surfaces without
  leaking TOML, terminal, or editor dependencies into parser/format/lint users.
- Internal workspace dependencies are versioned as well as path-based so every publishable crate can be packaged.
- Added a release-oriented `cargo xtask production-soak --corpus-root <path>` gate that runs parse, lint, format
  validation, idempotence, fmt-check comparison, and aggregate rewrite metrics over a primary documentation corpus and
  representative external Markdown fixtures.
- Added `cargo xtask mdformat-parity` to compare mdwright against mdformat on isolated corpus copies and require every
  output difference to be classified as fixed, configured, intentional, upstream-owned, or an open release-blocking bug.
- Added `cargo xtask parser-audit` source-position risk gates. The audit now checks cmark-gfm `data-sourcepos` envelopes
  against mdwright document facts for formatter/lint-owned constructs and fails on unclassified coordinate risks.
- Added `cargo xtask release-evidence --output target/mdwright/release`, which aggregates local release-candidate
  evidence into JSON and Markdown reports without wiring costly gates into CI.
- Added a document-owned GFM extension overlay for URL autolinks, email autolinks, and tagfiltering. Rendering now
  matches cmark-gfm for those extension cases by default, while the `bare-url` lint still asks authors to make URL
  autolinks explicit with `<...>` for renderer portability.
- Added `[render] profile = "pulldown" | "cmark-gfm"` and `mdwright render --render-profile`. The default keeps
  pulldown-style HTML output; the cmark-gfm profile matches cmark-gfm spelling for quote escaping, link-destination
  escaping, ordinary GFM tables, task-list checkboxes, and one raw-HTML newline case without changing parser semantics
  or formatter source bytes.
- Documented the remaining cmark-gfm parser-backend divergences as accepted current-backend constraints: emphasis
  delimiter-stack resolution, a few raw HTML block indentation cases, disabled task-list spec fixtures, and the
  contained upstream parser panic.
- The wrap pass now treats MkDocs-style `!!!` / `???` admonition paragraphs as opaque blocks, preventing prose wrapping
  from collapsing the admonition marker and indented body into ordinary paragraph text.
- Added `[fmt] profile = "preserve" | "mdformat"`. The `mdformat` profile keeps mdformat's default `wrap = keep`, uses
  dash bullets, safe repeated-`1.` ordered-list markers, 70-underscore thematic breaks, and padded GFM tables where
  transactional verification preserves document semantics. Explicit `[fmt]` keys override profile defaults.
- Added `fmt.tables.style = "preserve" | "pad"` for verified GFM table cell padding.
- Fixed dogfood lint false positives for inline-code plural suffixes, `jsonc` code fences, and MyST directive fences.
- Reworked formatter canonicalisation around rewrite families with explicit ownership. Inline delimiters and link
  destinations are slot-owned, list markers are marker-owned, table padding is a parent normal form, wrap is terminal,
  and idempotence is now checked by regression and property-law gates.
- Added `mdwright-latex` as the TeX math-body component. It owns MathJax-style command vocabulary, Unicode terminal
  layout, and source translation evidence; `mdwright-math` remains the Markdown math-span recogniser.
- Added `mdwright preview` Unicode math rendering and `mdwright math --to-unicode|--to-latex` source translation.
  Coverage targets common MathJax-style input where Unicode has honest representations; unsupported TeX reports typed
  diagnostics or falls back to source instead of pretending to be browser MathJax.
- Replaced Unicode-to-LaTeX substitution with a parser-backed source translator in `mdwright-latex`. Supported Unicode
  math source emits canonical LaTeX; unsupported glyphs, ambiguous accent ownership, and diagram-like source remain
  visible with diagnostics or losses.

### Performance

- Criterion comparison against the pre-factorisation baseline found no representative runtime regression above 10%.
  Formatter benches were flat to low-single-digit changes (`parse_plus_format/medium` flat; `format/corpus/wrap-100`
  about 1-2% faster). Lint-only paths improved by roughly 8-13%, while parse-plus-lint moved by about 3-5%. One
  tracing-disabled micro-format bench measured just under 10% slower but stayed below the investigation threshold and
  did not appear in representative parse-plus-format or corpus runs.

## [0.1.0] – 2026-05-18

First crates.io release. mdwright has been developed internally through a sequence of in-repo versions (0.1, 0.3, 0.4)
that were never published; this 0.1.0 is the first version external users can pin to. Pre-1.0 caveats apply; see
[reference/semver.md](https://jcreinhold.github.io/mdwright/reference/semver.html#pre-10-caveats) and the public-surface
snapshot at [reference/public-api.md](https://jcreinhold.github.io/mdwright/reference/public-api.html).

### Lint

- Standard library of fifteen rules under `mdwright::stdlib`: `adjacent-code-no-space`, `bare-url`, `duplicate-heading`,
  `duplicate-link-label`, `escaped-emphasis`, `heading-punctuation`, `inconsistent-list-marker`, `info-string-typo`,
  `latex-command`, `list-tightness-flipped`, `orphan-reference-link`, `stray-dollar`, `subscript-damage`,
  `trailing-whitespace`, `unbalanced-backtick`, `unicodeable-subscript`, plus three math rules
  (`math/unbalanced-braces`, `math/unbalanced-delim`, `math/unbalanced-env`). `RuleSet::stdlib_defaults` returns the
  curated default-on subset; `RuleSet::stdlib_all` returns the lot.
- Third-party rules via the [`LintRule`] trait and `RuleSet::add`. See `examples/extending/` for the canonical plugin
  recipe.
- Inline suppression comments: `<!-- mdwright: allow ... -->`, `<!-- mdwright: allow-next-line ... -->`,
  `<!-- mdwright: disable [...] -->` / `enable`, `disable-all` / `enable-all`.

### Format

- Math-resilient round-trip formatter. Math regions (`\[…\]`, `$$…$$`, `\(…\)`, `$…$`, and LaTeX environments) are
  preserved verbatim by default; the structural recogniser at `mdwright::cm::math` uses the IR's inline and block atoms,
  including inline HTML, as exclusion zones, closing the "stray `$` anchors a phantom math region" class of false
  positive. Dollar variants remain opt-in via `MathConfig`.
- Preserve-by-default style knobs. `italic`, `strong`, `list_marker`, `thematic_break_style`, `ordered_list`,
  `link_def_style` all default to `Preserve`; the default formatter leaves source style unchanged. Each knob has an
  opt-in canonicalising target (`asterisk`, `underscore`, `dash`, etc.) configurable via `.mdwright.toml` or
  programmatically through fluent setters (`with_italic`, `with_strong`, `with_list_marker`, `with_ordered_list`,
  `with_thematic_break`, `with_link_def_style`).
- `StrongStyle` is independent of `ItalicStyle`. `*italic*` with `__strong__` is expressible (`[fmt] strong = "asterisk"
  | "underscore" | "preserve"`, default `preserve`).
- `--math-render={none, commonmark-katex, dollar}` flag on `fmt` plus a `mdwright render` subcommand for one-shot
  conversion. The converter at `src/cm/math/render.rs` lifts LaTeX-flavoured delimiters to CommonMark-compatible `$…$` /
  `$$…$$` for downstream KaTeX rendering.
- Math pretty-printer at `mdwright::cm::math::pretty`. Whole-block math regions are normalised: whitespace inside the
  body, opener / closer on their own lines, and `&` columns inside aligning environments padded to per-column Unicode
  display width. Gated behind `FmtOptions::math().normalise` (default `false`) because pulldown parses math bodies as
  prose; opt-in for authors with a downstream math renderer.
- MyST + Pandoc directive preservation: directive containers (`:::{name}`), fenced divs (`::: {.cls}` / `:::name`),
  inline roles (`` {role}`payload` ``), substitutions (`{{name}}`), Pandoc inline attribute spans (`[content]{.cls}`),
  and `%` line comments. The first inline overlay lives at `src/format/inline.rs::apply_inline_overlay`; idempotence is
  gated by `tests/external_corpora.rs` against the vendored jupyter-book demo.
- mdformat-mkdocs parity: definition lists and heading attribute lists via pulldown events; abbreviations and
  non-heading block attribute lists as scan-and-preserve overlays. Defaults on. Inline attribute lists are explicitly
  out of scope. Byte parity gated by `tests/extension_parity.rs`.
- Line wrap: `Wrap::Keep` / `Wrap::No` / `Wrap::At(n)`. The Knuth-Plass-lite DP at `src/format/wrap.rs` is bounded
  (paragraphs > 100 000 boxes skip the DP and emit verbatim; 100 ms time budget).
- Frontmatter preservation: YAML (`---`) and TOML (`+++`) opening fences. `Frontmatter::delimiter` is read back through
  `Document::frontmatter`.
- Range formatting: `format_range` and `format_range_with_checkpoints` re-emit the smallest set of whole top-level
  blocks covering a caller-supplied byte range. Substring contract fenced by
  `tests/properties.rs::range_format_is_substring_of_whole`.

### CLI

- Subcommands: `check`, `fmt`, `fmt-check`, `fix`, `list-rules`, `explain`, `render`, `lsp`. Run `mdwright --help` for
  the full surface; see [`reference/cli.md`](https://jcreinhold.github.io/mdwright/reference/cli.html).
- `mdwright explain <rule>` prints the long-form prose for a rule. Unknown names get a Jaro-Winkler "did you mean?"
  suggestion.
- Rustc-style pretty diagnostic output: severity header (`error[rule]:`, `advisory[rule]:`, …), `--> path:line:col`
  location, source snippet with caret underline, `help:` line drawn from the rule's `explain()`, optional `fix:`
  preview, and a `note: see mdwright explain <rule>` footer. Coloured by `owo-colors` when stdout is a TTY; controlled
  by `--color=always|never|auto`.
- JSON Lines v2 schema at
  [`docs/diagnostic-schema.json`](https://github.com/jcreinhold/mdwright/blob/main/docs/src/reference/diagnostic-schema.json)
  with prose at
  [`reference/diagnostic-schema.md`](https://jcreinhold.github.io/mdwright/reference/diagnostic-schema.html). Records
  carry `schema_version: 2`, `severity`, a nested `rule` object with `url` into the per-rule pages, and a `source`
  object with the offending line text. v1 remains available under `--format=json-v1` for one release cycle; a
  deprecation warning is printed to stderr.
- `--max-input-bytes` (default 10 MB) caps the size of any single file or stdin payload. Pass `0` to disable.
- `--reject-control-chars` opts into rejecting C0 control bytes (other than TAB, LF, FF, CR) that pulldown would
  silently rewrite.
- `--mode={normalise,verbatim}` selects the formatter dispatch policy.
- `--verbose` / `-v` count-flag controls the `tracing` log level. Logs are silent by default; `-vvv` shows per-construct
  decisions.

### Editor integration

- `mdwright lsp` subcommand: a built-in Language Server Protocol server (stdio transport, `tower-lsp` backend) exposing
  diagnostics, code actions for safe autofixes, hover docs sourced from `mdwright explain`, and
  `textDocument/formatting` / `rangeFormatting` / `onTypeFormatting`. Editor recipes for Helix, Zed, VS Code, and Neovim
  live at
  [`integration/editor-integrations.md`](https://jcreinhold.github.io/mdwright/integration/editor-integrations.html).

### Configuration

- `.mdwright.toml` / `mdwright.toml` / `pyproject.toml [tool.mdwright]`. `Config::discover` walks ancestors until it
  hits a `.git/` boundary; `Config::load_explicit` loads a single named file.
- Schema documented at [`configuration.md`](https://jcreinhold.github.io/mdwright/configuration.html) (auto-generated by
  `build.rs` from a single source of truth).
- `Config::defaults()` is a synchronous constructor for the all-defaults config; used by the LSP server when discovery
  encounters an unreadable config file mid-walk.

### CI integrations

- `.pre-commit-hooks.yaml` at the repo root exposes six hook IDs for the [`pre-commit`](https://pre-commit.com)
  framework: `mdwright-check` / `mdwright-fmt` / `mdwright-fmt-check` built from source via `language: rust`, and
  `*-system` variants that invoke a pre-installed binary on `$PATH`.
- `action.yml` at the repo root: a composite GitHub Action that downloads the matching release tarball
  (`x86_64-unknown-linux-gnu` or `aarch64-apple-darwin`) and runs `mdwright` with caller-provided `args`. Usage:
  `uses: jcreinhold/mdwright@v0.1.0`.
- `examples/downstream/`: minimal end-to-end fixture (good + intentionally bad Markdown files plus a
  `.pre-commit-config.yaml`) exercised by `tests/downstream_integration.rs`.
- `cargo xtask bump-docs-version`: rewrites `rev: vX.Y.Z` and `@vX.Y.Z` pins in integration docs and example configs to
  match `Cargo.toml`'s `[package].version`. Drift gated in CI by `tests/integration_versions_in_sync.rs`.

### Documentation

- mdBook site at <https://jcreinhold.github.io/mdwright/> deployed by `.github/workflows/docs.yml`.
- Per-rule pages at `docs/rules/<name>.md` generated by `cargo xtask doc-rules`. CI test `rule_docs_in_sync` fails on
  drift.
- CLI reference at [`reference/cli.md`](https://jcreinhold.github.io/mdwright/reference/cli.html) generated by
  `cargo xtask doc-cli`.
- Public-API surface snapshot at
  [`reference/public-api.md`](https://jcreinhold.github.io/mdwright/reference/public-api.html). Pre-1.0, this snapshot
  is descriptive; see the [Pre-1.0 caveats](https://jcreinhold.github.io/mdwright/reference/semver.html#pre-10-caveats)
  in the semver policy.

### Architecture

- IR is spec-aligned: each CM/GFM construct is a typed Rust value whose constructor enforces well-formedness. Spec
  conformance is a construction-time property rather than a runtime sieve.
- The `format::*` pipeline dispatches per-construct `pretty()` methods through `TypedBlock::pretty`. Structural emit is
  a single pass with pure source-byte preservation (`src/format/document.rs`): every `.pretty()` reads source bytes
  through `Tree::raw_text` or a node's source-recorded field; none consult `FmtOptions` style knobs. Idempotent by
  construction.
- Style canonicalisation runs as a separate post-structural pass at `src/format/canonicalise.rs`. Each rewrite is
  verified locally by reparsing the affected paragraph window through the parse chokepoint (`src/parse.rs::events`);
  failed verifications skip silently with `tracing::warn!`. The pass iterates to a fixed point (cap
  `MAX_CANONICALISE_ITERS = 8`).
- Math recognition lives at `mdwright::cm::math` and runs before the tree build so the IR sees math as opaque atoms.
- Plugin extension model: `mdwright::cli::run_with_rules(RuleSet) -> ExitCode` lets downstream binaries embed mdwright
  with extra lint rules over the published IR. Dynamic / WASM plugin loading is explicitly deferred (see
  [`extending/plugin-loading.md`](https://jcreinhold.github.io/mdwright/extending/plugin-loading.html)).
- See [`format/policy.md`](https://jcreinhold.github.io/mdwright/format/policy.html) (user-facing) and
  `docs/architecture/stability.md` (contributor-facing) for the design.

### Testing & QA

- Property tests at `tests/properties.rs` matrix every style knob: canonicalisation modes × {byte idempotence,
  `semantically_equivalent`} on per-construct generators plus a whole-document sweep at 4096 cases (`#[ignore]`-gated).
- Coverage-guided fuzz harness at [`fuzz/`](./fuzz) with three targets: `fuzz_parse_format`, `fuzz_idempotence`,
  `fuzz_lint`. See [README §Safety](./README.md#safety).
- Full CommonMark / GFM spec runner at `tests/gfm_spec.rs` with a snapshot mechanism for documented deviations; see
  [`deviations.md`](https://jcreinhold.github.io/mdwright/deviations.html).
- Cross-platform CI matrix: Linux / macOS / Windows × {stable, MSRV floor 1.91}. `.gitattributes` forces LF.
- `tests/discover_symlink_loop.rs` pins the symlink-handling contract; `discover_markdown` does not follow symlinks and
  so is immune to symlink loops.
- `SECURITY.md` disclosure template.

### Distribution

- cargo-dist release pipeline at `.github/workflows/release.yml` builds binaries and a shell installer for
  `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin`. `binstall` metadata in `Cargo.toml` pins the `.tar.xz` URL
  pattern.
- MSRV: `rust-version = "1.91"`, edition 2024 (edition floor 1.85).

### Known issues

- Idempotence regression on paragraphs whose lines are separated by a form-feed-only line: the formatter re-splits such
  paragraphs so a continuation-line `+` ends up at a block-start position on reparse, promoting it to an empty
  bullet-list marker. Class is "block-boundary classification disagrees between once and twice when a whitespace-only
  line carries non-blank-line whitespace (form-feed, line tabulation, …)." Reproducer at
  [`fuzz/known-issues/idempotence-formfeed-paragraph-resplit.in`](./fuzz/known-issues/README.md).
