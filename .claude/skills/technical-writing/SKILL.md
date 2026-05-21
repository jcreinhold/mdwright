---
name: technical-writing
description: 'Use for writing, revising, or reviewing prose: docstrings, comments, mathematical exposition, issues, design docs, PR summaries, READMEs, commit messages.'
---

# Technical Writing

Help the reader understand. Everything else serves that.

## When to use

- Writing or revising prose in `docs/`, design documents, or READMEs
- Improving comments and docstrings for clarity and rationale
- Drafting or editing issues, PR summaries, blocker reports, commit messages
- Any text a human will read and that matters

## What to read first

Before editing prose, read [references/on-writing.md](./references/on-writing.md) — universal
prose craft, then technical documentation, then comments. It is the baseline; local
convenience does not override it.

## Workflow

### Edit (default)

1. Read the target text and enough surrounding context to know what must not change — the
   root `CLAUDE.md` and any nested guide that applies, the local `README.md`, and adjacent
   code or docs that the text refers to.
1. Read the reference above.
1. Identify the audience and the purpose. A docstring serves callers; a design doc serves
   reviewers; a commit message serves the next person who blames the line.
1. Find the real problems: buried lede, vague pronouns, jargon used without inline
   definition, redundancy, comments that restate code, paragraphs doing more than one job.
1. Rewrite. Preserve every claim and technical distinction exactly. Change structure, word
   choice, sentence construction, paragraph breaks. Add a brief inline definition the
   first time a term appears. Replace a code-restating comment with rationale, or delete
   it.
1. Show the before/after for localized changes. For substantial rewrites, show the revised
   text with a short note on what changed and why.

### Review only

When asked to review without editing, report each problem in place — location, what is
wrong, how to fix it — and stop. Reporting is the deliverable.

## mdwright-specific rules

- **Don't hand-edit generated pages.** `docs/src/rules/*.md` and `docs/src/rules/index.md`
  are emitted by `xtask::page_for` and `xtask::index_page` in `xtask/src/lib.rs`. The
  schema table in `docs/src/configuration.md` is bounded by `<!-- BEGIN GENERATED -->`
  markers from `cargo xtask doc-config`. Hand edits get clobbered and the drift gate
  fails. Edit the generator, then run `cargo run -p xtask -- doc-rules` (or `doc-config`).
- **mdwright lints its own prose.** After editing a doc page or README, run
  `mdwright check docs/ README.md crates/*/README.md` and `mdwright fmt-check` over the
  same set. The default rule catalogue is the project's writing taste; passing it is the
  local "did I leave things tidy" check.
- **Discipline rules from `CLAUDE.md` apply to prose too.** No `TODO`s or placeholders in
  shipped docs; no metacommentary referencing deleted content ("this section used to
  cover X" belongs in the PR description, not the page).

## Subagents

For minimal prose-only edits, a local worker is available:

- [`agents/edit.md`](./agents/edit.md) — reads the reference, identifies prose problems,
  rewrites for clarity and precision.
