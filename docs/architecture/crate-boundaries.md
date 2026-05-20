# Crate boundaries

mdwright is a virtual workspace. Each crate hides a different volatile decision; library users depend directly on the
component crate they need. The repository root is a workspace manifest, not a package—there is no facade library and no
root binary.

```text
Cargo.toml               # virtual workspace root, no package targets
crates/mdwright          # command-line package and `mdwright` binary
crates/mdwright-document # parsed Markdown facts with stable source coords
crates/mdwright-math     # TeX/math span recognition, rendering, normalisation
crates/mdwright-format   # formatter policy, rewrite-family planning, oracles
crates/mdwright-lint     # diagnostics, rule execution, suppression, safe fixes
crates/mdwright-config   # TOML schema, discovery, resolved option construction
crates/mdwright-lsp      # tower-lsp server and editor-state bridge
```

Dependency direction:

```text
       mdwright-math
            │
       mdwright-document
         │         │
mdwright-format   mdwright-lint
         │         │
         mdwright-config
            │
   mdwright / mdwright-lsp
```

## What each crate owns

| Crate              | Hides                                                                                      |
| ------------------ | ------------------------------------------------------------------------------------------ |
| `mdwright-math`    | TeX delimiter and environment recognition; math body normalisation.                        |
| `mdwright-document`| CommonMark/pulldown quirks, GFM extension overlays, source-coordinate invariants, parser-panic containment. Owns the only production `pulldown-cmark` chokepoint. |
| `mdwright-format`  | Formatter style policy, rewrite-family planning, local ownership checks, semantic verification. |
| `mdwright-lint`    | Rule dispatch, suppressions, diagnostic shape, safe-fix edit ordering, standard-rule registry. |
| `mdwright-config`  | TOML schema and discovery rules; resolves into the per-crate option types.                 |
| `mdwright`         | File discovery, argument parsing, terminal output, parallel execution, exit policy.        |
| `mdwright-lsp`     | Editor-state delivery over LSP.                                                            |

The document crate is a parse/query abstraction; formatting and linting are operations owned by the crates that hide
their algorithms. Other crates consume document facts as domain records (structural spans, paragraphs, list-marker
sites, inline delimiter slots, heading attribute trailers, link destination slots, math regions, frontmatter, code/HTML
exclusions, top-level checkpoints) and do not couple to pulldown's event vocabulary, offset iterator, panic payloads, or
backtraces.
`pulldown_model` tests may import pulldown directly because they deliberately probe upstream drift.

## Public API entry points

- `mdwright_document::Document::parse_with_options(source, ParseOptions)
   -> Result<Document, ParseError>` parses
  fallibly at the parser trust boundary. The returned `Document` stores its `ParseOptions`; formatter entry points
  read that policy from the `Document` so every rewrite, semantic signature, verification reparse, and range-format
  checkpoint uses the same recognition.
- `mdwright_format::{format_document, format_validated}` over a parsed `Document`.
- `format_source(source, opts)` is the convenience path for default parse policy.
- `mdwright_lint::RuleSet::{check, check_with}` for lint dispatch; `mdwright_lint::apply_safe_fixes` for safe-fix
  application.
- The `mdwright` package exposes command-extension helpers such as `run_with_rules` but is otherwise a binary, not a
  library.

`ExtensionOptions`, `MystOptions`, and `PandocOptions` are document parse policy under `ParseOptions`. GFM extension
policy is `parse.extensions.gfm.autolinks` and `parse.extensions.gfm.tagfilter`; the document crate exposes autolinks as
general `AutolinkFact` values rather than URL-specific GFM facts. HTML render spelling is document-owned policy exposed
as `[render] profile`.

## Dependency fences

Enforced by `crates/mdwright/tests/dependency_fences.rs` via `cargo tree`:

- `mdwright-math` depends on no other `mdwright-*` crate.
- `mdwright-document` may depend on `mdwright-math`; it must not depend on format, lint, config, CLI, LSP, `clap`,
  `ignore`, `rayon`, `serde`, `toml`, `tokio`, `tower-lsp`, `owo-colors`, or `anyhow`.
- `mdwright-format` may depend on `mdwright-document` and `mdwright-math`; it must not depend on lint, CLI, LSP, `clap`,
  `tokio`, or `tower-lsp`. It does not import `pulldown-cmark` or `mdwright_document::parse` in production code.
- `mdwright-lint` depends on `mdwright-document`; it must not depend on format, CLI, LSP, `clap`, `tokio`, or
  `tower-lsp`.
- `mdwright-config` may depend on document/format/lint option types; it must not depend on CLI or LSP.
- `mdwright` and `mdwright-lsp` are delivery crates; heavy delivery dependencies belong there.
- `mdwright-document` does not publicly export parser helpers.
- Config schema and docs hold recognition keys under `[parse.extensions]`, not formatter policy.
- Workspace-internal dependencies carry both `path` and `version`.

## Packaging

Every publishable crate uses versioned internal dependencies with local `path` entries for development. Cargo
strips paths during packaging while local workspace builds keep using the checked-out crates. The repository
`.cargo/config.toml` patches internal packages to local paths so local `cargo package --no-verify` checks run before
those packages exist on crates.io. Publishing order:

1. `mdwright-math`
2. `mdwright-document`
3. `mdwright-format`, `mdwright-lint`
4. `mdwright-config`
5. `mdwright-lsp`
6. `mdwright`

## Why these crates, not others

- No `mdwright-source` / `mdwright-source-map` / `mdwright-text`: source canonicalisation and byte mapping are part
  of the document abstraction. Callers want a recognised document whose spans map back to user bytes, not a separate
  coordinate package.
- No `mdwright-util`: a utility crate has no domain responsibility and becomes a junk drawer.
- No `mdwright-rules`: standard rules and rule dispatch share suppression, diagnostic, and registry semantics;
  separating them would mirror an old directory layout shallowly.
- No root facade package and no root `Document` newtype to preserve `doc.format()` / `doc.lint()`: either wrapper would
  expose the same abstraction as `mdwright-document::Document` and add no hiding.

The alternative was a larger `mdwright-engine` crate that owned `Document`, lint, format, and safe-fix operations. That
keeps formatter and linter complected with document recognition: one central crate would know parser byte ranges, lint
suppression semantics, formatter transactional verification, standard-rule registration, and safe-fix edit ordering. The
current split makes a new formatter rewrite touch `mdwright-format` plus tests; a new lint rule touch `mdwright-lint`
plus docs; a new config key touch `mdwright-config` plus the option type it resolves; a new CLI flag not drag parser,
formatter, or lint internals into the CLI surface.
