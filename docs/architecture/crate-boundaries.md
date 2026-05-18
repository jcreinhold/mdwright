# Crate Boundaries

This document records the crate split that turns `mdwright` from one historical implementation crate into a workspace
of deeper components. The split follows the knowledge each crate owns, not the order in which the tool happens to run.

## Current Coupling Symptoms

The single crate currently braids independent concerns:

- `Document` is both a parsed fact handle and an operation host for linting, formatting, validated formatting, and safe
  fix application.
- `FmtOptions` owns Markdown extension-recognition toggles, even though those toggles decide what the parser recognises,
  not how formatting rewrites source bytes.
- Formatter verification reaches into parser, tree, math, reference, and semantic comparison internals through
  `crate::` paths, so byte-rewrite safety is a formatter concern implemented with document-private knowledge.
- Lint dispatch, suppression handling, standard-rule registration, diagnostics, and safe-fix application are split
  across top-level modules and `Document` methods.
- CLI and LSP delivery pulled in heavy dependencies (`clap`, `ignore`, `rayon`, `tokio`, `tower-lsp`, terminal colour)
  through the root library dependency set, so every library user paid for delivery concerns.
- Config parsing, TOML schema validation, discovery, formatter policy, lint policy, and document-recognition policy all
  share one module.

The audit command found 164 direct `crate::` imports across these concerns. Recent history also shows volatile changes
in document recognition, math recognition, formatter rewriting, extension overlays, LSP delivery, and config shape. The
old single crate gave those decisions no crate-level hiding boundary.

## Design A: Decomplected Capability Crates

Design A splits by stable capability and hidden knowledge:

- `mdwright-document`: recognised Markdown document facts with stable coordinates back to original user bytes.
- `mdwright-math`: pure TeX/math span recognition, errors, rendering helpers, and body normalisation.
- `mdwright-format`: formatting policy, range formatting, semantic formatter oracles, and the transactional rewrite
  engine.
- `mdwright-lint`: diagnostics, rule execution, suppression, standard rules, and safe fixes.
- `mdwright-config`: raw TOML schema, config discovery, and conversion into resolved document/format/lint options.
- `mdwright-cli`: file discovery, argument parsing, terminal output, parallel file execution, and exit policy.
- `mdwright-lsp`: editor-state delivery over LSP.
- `mdwright`: curated library facade.

This design makes the document crate a parse/query abstraction only. Formatting and linting become operations owned by
the crates that hide their algorithms. The root facade preserves the common user-facing imports without turning back
into an implementation or delivery crate.

## Design B: Deep Engine Plus Delivery Crates

Design B would keep a larger `mdwright-engine` or `mdwright-document` crate that owns `Document`, lint, format, and safe
fix operations. CLI, LSP, and config would move out, but parser facts, lint rules, and formatter rewrites would remain
together.

This preserves more old fluent syntax (`doc.lint(...)`, `doc.format(...)`) and moves fewer files. It also keeps the
same abstract problem: one central crate would continue to know parser byte ranges, lint suppression semantics,
formatter transactional verification, standard-rule registration, and safe-fix edit ordering. Adding a formatter
rewrite or lint rule would still compile in the same dependency universe.

## Comparison

Design A has more crates, but each crate hides a different volatile decision:

- CommonMark/pulldown quirks and source-coordinate invariants are hidden in `mdwright-document`.
- TeX delimiter and environment recognition is hidden in `mdwright-math`.
- Byte rewrite ordering, overlap rejection, fixed-point iteration, and semantic verification are hidden in
  `mdwright-format`.
- Rule dispatch, suppressions, diagnostics, and fixes are hidden in `mdwright-lint`.
- Raw TOML schema and discovery rules are hidden in `mdwright-config`.
- File-system/terminal policy and editor-server policy are separated into delivery crates.

Design B is easier to implement, but it keeps formatter and linter operations complected with document recognition.
That is the wrong tradeoff for the next changes: formatter bugs should be fixed inside formatter transactions, lint
rules should consume immutable facts, and document parsing should not know who will use the facts.

Design A wins on depth, information hiding, and lower change amplification. A new formatter rewrite should touch
`mdwright-format` plus tests. A new lint rule should touch `mdwright-lint` plus docs. A new config key should touch
`mdwright-config` and the option type it resolves. A new CLI flag should not drag parser, formatter, or lint internals
into the CLI surface.

## Chosen Design

The workspace uses Design A.

```text
mdwright                 # curated library facade
crates/mdwright-document # recognised Markdown facts with stable coordinates back to original user bytes
crates/mdwright-math     # pure TeX/math span recognition, errors, rendering helpers, and body normalisation
crates/mdwright-format   # formatter policy, range formatting, transactional rewrite engine, semantic oracles
crates/mdwright-lint     # diagnostics, lint rules, suppression, safe fixes, stdlib registry
crates/mdwright-config   # TOML schema, discovery, resolved option construction
crates/mdwright-cli      # clap, file discovery, terminal output, process exit policy
crates/mdwright-lsp      # tower-lsp server and editor-state bridge
```

Dependency direction:

```text
mdwright-math
      ^
      |
mdwright-document
      ^            ^
      |            |
mdwright-format   mdwright-lint
      ^             ^
      |             |
      +---- mdwright-config
                  ^
                  |
       mdwright-cli / mdwright-lsp

mdwright facade depends on document/format/lint/config only
```

The executable named `mdwright` is owned by the `mdwright-cli` package. The root package deliberately has no binary
target: normal library users should not resolve CLI/LSP dependencies or delivery transitive dependencies.

## Rejected Crates

- No `mdwright-source`, `mdwright-source-map`, or `mdwright-text`: source canonicalisation and original/canonical byte
  mapping are part of the document abstraction. Callers want a recognised document whose spans map back to user bytes,
  not a separate coordinate package.
- No `mdwright-util`: a utility crate would have no domain responsibility and would become a junk drawer for avoiding
  better boundaries.
- No `mdwright-rules` in this pass: standard rules and rule dispatch share suppression, diagnostic, and registry
  semantics; separating them now would make a shallow mirror of an old directory.
- No root `Document` newtype to preserve `doc.format()` / `doc.lint()`: that wrapper would expose the same abstraction
  as `mdwright-document::Document` and add no hidden implementation.

## Public API Breaks

The API is pre-1.0, so the split removes operation methods from `Document` where keeping them would create cycles:

- `Document::format` and `Document::format_validated` move to `mdwright_format::{format_document, format_validated}`
  and root-facade reexports.
- `Document::lint` and `Document::lint_with` move to `RuleSet::{check, check_with}`.
- `Document::apply_safe_fixes` moves to `mdwright_lint::apply_safe_fixes`.
- `ExtensionOptions`, `MystOptions`, and `PandocOptions` become document parse policy under `ParseOptions`, not
  formatter policy under `FmtOptions`.
- The CLI binary moved from the root package to `mdwright-cli`; the binary name remains `mdwright`.
- The configuration table for recognition toggles moved from formatter policy to `[parse.extensions]`.

The root `mdwright` facade re-exports the intended user-facing types and free functions. It does not re-export
low-level document representation (`Source`, `Tree`, `Node`, parser helpers), file discovery, CLI entry points, or LSP
entry points. Integration tests and examples should prefer the facade unless they intentionally test an internal crate.

## Parse Policy Flow

Config resolves recognition toggles into `mdwright_document::ParseOptions`. A parsed `Document` stores the
`ParseOptions` used to build it. Document-aware formatter entry points read that policy from the `Document`, and every
rewrite snapshot, semantic signature, verification reparse, range-format checkpoint table, and fixed-point iteration
uses the same policy.

`format_source(source, opts)` is the convenience path for default parse policy. Callers that need non-default
recognition parse a `Document` with `Document::parse_with_options` and call `format_document`, `format_validated`, or
range formatting over that document.

## Parser Boundary

`mdwright-document` owns the only production `pulldown-cmark` parser chokepoint. Other crates consume document facts
with stable source ranges: structural spans, paragraphs, list marker sites, heading attribute trailers, inline and
reference-link destination ranges, math regions, frontmatter, code/HTML exclusions, and top-level block checkpoints.
Those facts are domain records, not pulldown event wrappers, so formatter and lint crates do not couple to pulldown's
event vocabulary or offset iterator.

`pulldown_model` tests may still import `pulldown-cmark` directly because they deliberately test upstream parser drift.

## Packaging Contract

Every publishable workspace crate uses versioned internal dependencies with local `path` entries for development. Cargo
therefore has enough information to strip paths during packaging while local workspace builds keep using the checked-out
crates. The repository `.cargo/config.toml` patches internal packages to local paths so local `cargo package
--no-verify` checks can run before those packages exist on crates.io. The intended publishing order is:

1. `mdwright-math`
2. `mdwright-document`
3. `mdwright-format` and `mdwright-lint`
4. `mdwright-config`
5. `mdwright-lsp` and `mdwright-cli`
6. root `mdwright` facade

## Dependency Fences

These fences are part of the implementation contract:

- `mdwright-math` has no dependency on any other `mdwright-*` crate.
- `mdwright-document` may depend on `mdwright-math`; it must not depend on format, lint, config, CLI, LSP, `clap`,
  `ignore`, `rayon`, `serde`, `toml`, `tokio`, `tower-lsp`, `owo-colors`, or `anyhow`.
- `mdwright-format` may depend on `mdwright-document` and `mdwright-math`; it must not depend on lint, CLI, LSP, `clap`,
  `tokio`, or `tower-lsp`.
- `mdwright-lint` depends on `mdwright-document`; it must not depend on format, CLI, LSP, `clap`, `tokio`, or
  `tower-lsp`.
- `mdwright-config` may depend on document/format/lint option types; it must not depend on CLI or LSP.
- `mdwright-cli` and `mdwright-lsp` are delivery crates. Heavy delivery dependencies belong there.
- The root `mdwright` crate is a library facade, not a home for parser, formatter, linter, config, CLI, or LSP
  implementation files, and not the owner of the executable.
- `mdwright-format` does not import `pulldown-cmark` or `mdwright_document::parse` in production code.
- `mdwright-document` does not publicly export parser helpers.
- Config schema and docs contain recognition keys under `[parse.extensions]`, not formatter policy.
- Workspace internal dependencies carry both `path` and `version`.

CI-visible checks enforce these with `cargo tree` in `tests/dependency_fences.rs`.
