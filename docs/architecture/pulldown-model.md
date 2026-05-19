# Pulldown-cmark model

Reference for the per-construct behaviours of `pulldown-cmark` 0.13 that
mdwright depends on. Every emit-site decision in `src/format/` either
matches a rule on this page or contradicts pulldown—the latter is a
bug.

This file is paired with `tests/pulldown_model.rs`. Each rule below
has one test in that file that feeds the documented example to
pulldown and asserts the documented event-stream shape. When pulldown
changes upstream (a release bump, a bug fix on their side), the test
fails and **this document must be updated before any mdwright code is
changed in response**.

Every production parse in `src/` flows through `src/parse.rs::events`
or `events_with_offsets`, both of which take a `CanonicalSource<'_>`
(`crate::source`). That type's only public constructor routes through
`Source::canonicalise`, so pulldown's input is *always* CR-free and
NUL-free in production. Rules below assume that pre-condition.

## §1 Line endings

`Source::canonicalise` strips CR / CRLF → LF and NUL → U+FFFD before
pulldown sees the buffer (CM §2.1, §2.3). Inside HTML blocks, code
blocks, math regions, and inline code, pulldown preserves the (now-LF)
bytes verbatim in the `CowStr` payload. In prose, a single `\n` between
non-blank content lines becomes `Event::SoftBreak`; two consecutive
`\n`s end the current block.

Consequence: no `CowStr` produced by `Event::Text`, `Event::Code`,
`Event::Html`, `Event::InlineHtml`, `Event::InlineMath`, or
`Event::DisplayMath` can ever contain a CR byte in production. The
semantic-equivalence walker (`src/format/semantic.rs::canonical_events`)
relies on this—there is no per-event CR scrub.

Test: `line_endings_softbreak_between_lines`.

## §2 Trailing blank lines in containers

Pulldown strips trailing blank lines from indented code blocks before
emitting the final `Event::Text`. A whitespace-only line is "blank."

The source `"\t|\n\t"` produces a single `Event::Text("|\n")` inside
the indented code block: the trailing tab-only line is consumed as a
blank line, but the terminating `\n` of the *content* line stays in
the payload. The formatter's `normalize_trailing_newline` consumes
that trailing LF when re-emitting; without it the formatter would
emit one trailing LF too many.

Cite: regression fixture `tests/regressions/fuzz_indented_code_trailing_ws_drop.in`.

Test: `indented_code_keeps_content_terminating_newline`.

## §3 Emphasis pairing scope

CM §6.2 / §6.3: emphasis delimiters pair *within their enclosing
pairing container*. The set of pairing containers pulldown observes:
paragraph, heading, table cell, link body, image body, footnote
definition.

Strikethrough (`~~…~~`) is **not** a pairing container—emphasis
delimiters can open inside one strikethrough run and close inside
another, or across a strikethrough boundary entirely. The safety
ladder's per-construct reparse takes this into account by including
the surrounding bytes in its flanking-context window.

Link bodies *are* a pairing boundary because CM §6.5 gives link text
grouping higher precedence than emphasis grouping. The two are not
symmetric: `*[foo*](bar)` parses with the `*` not pairing (it's
outside the link, the link doesn't enclose it), but the link text
`[foo*]` does not contribute to an outer `*…*` pair either.

Test: `emphasis_pairs_within_paragraph` and
`emphasis_pairs_across_strikethrough` and `link_body_breaks_emphasis_pairing`.

## §4 Reference label normalisation

CM §4.7: trim leading and trailing whitespace; collapse internal runs
of whitespace to a single U+0020; case-fold via Unicode default case
folding. Two labels resolve to the same definition iff their
normalised forms agree.

Pulldown 0.13 does **not** emit a `LinkReferenceDefinition` event.
Definitions are resolved internally during parse, and reference uses
surface as `Tag::Link { id: ".." }` where `id` is the *raw label
bytes* the source used (not the normalised form). The mdwright-side
authoritative scan for definitions lives in
`src/cm/refs.rs::build_reference_table`; that module is the sole site
that runs CM §4.7 normalisation.

Test: `reference_label_normalisation_matches`.

## §5 HTML block boundaries

CM §4.6 defines seven HTML block types, each with its own start /
end conditions. Two of the important asymmetries:

- **Type 2** (`<!-- … -->` or `<?…?>` style with a multi-char end
  marker): the block ends at the *line containing* the matching end
  marker (or EOF). The block's events are a sequence of
  `Event::Html(line)` per source line, each payload including its
  trailing newline—except possibly the last, which can omit the
  newline if the source did.
- **Type 6** (recognised tag names like `<table>`): the block ends at
  the first blank line after the start (or EOF). Recognition is by
  tag *name*, not by close-tag matching: `<table>` opens a type-6
  block; the close `</table>` does not by itself end it—a blank
  line does.

The block's payload bytes round-trip verbatim (modulo §1
canonicalisation), so the formatter emits HTML blocks by stamping the
captured source slice rather than reconstructing from events.

Test: `html_block_type2_emits_per_line_events`.

## §6 Emphasis-event range semantics

`Event::Start(Tag::Emphasis)` and `Event::End(TagEnd::Emphasis)` ranges
in the offset iterator cover the **entire run**—from the byte
position of the first character of the opening delimiter, to the byte
position *after* the last character of the closing delimiter.

- `range.start` of `Start(Emphasis)`: index of the first `*` or `_` of
  the opening run.
- `range.end` of `End(Emphasis)`: index *after* the last `*` or `_`
  of the closing run.
- The body bytes occupy `[start_range.end, end_range.start)`.

Same convention for `Strong`. The safety ladder
(`src/format/emit_safety.rs::parses_with_outer_run_at`) tests
`range.start == target_open` and `range.end == target_close` to
identify the candidate run—a pulldown change to either convention
would silently break the test, which is exactly the kind of drift the
model test catches.

Test: `emphasis_event_range_spans_delimiters`.

## §7 Strong vs nested emphasis disambiguation

CM §6.5 disambiguates runs of two through six `*` / `_` characters:

- `**foo**` → `Start(Strong)`, `Text("foo")`, `End(Strong)`. Not
  emphasis-of-emphasis.
- `***foo***` → `Start(Strong)`, `Start(Emphasis)`, `Text("foo")`,
  `End(Emphasis)`, `End(Strong)` (the nesting order depends on
  pairing direction; pulldown's left-flank rule decides).
- `*_foo_*` → `Start(Emphasis)`, `Start(Emphasis)`, `Text("foo")`,
  `End(Emphasis)`, `End(Emphasis)`. Two distinct delimiter characters
  pair independently.

The safety ladder predicate exists to keep the formatter from
collapsing these into each other when a body or neighbour byte change
would let pulldown re-segment differently.

Test: `strong_distinct_from_nested_emphasis`.

## §8 Definition-list event shape

With `Options::ENABLE_DEFINITION_LIST` set on the parser, the source

```
Term
: defn
```

emits the nested triple `Start(DefinitionList)` → `Start(DefinitionListTitle)`
→ … → `End(DefinitionListTitle)` → `Start(DefinitionListDefinition)` →
… → `End(DefinitionListDefinition)` → `End(DefinitionList)`. Each
definition's body is opened/closed independently, so a definition
containing multiple paragraphs emits multiple `Start(Paragraph)` /
`End(Paragraph)` pairs inside one `DefinitionListDefinition`.

The tree builder's `kind_for_start` arm relies on this nesting shape
to construct `NodeKind::DefinitionList` / `NodeKind::DefinitionTerm` /
`NodeKind::DefinitionDescription` and on `close_container`'s child
draining to thread terms and definitions into the typed `DefinitionList`
block at `src/cm/block/definition_list.rs`.

Test: `definition_list_emits_tag_triple`.

## §9 Heading attribute fields

With `Options::ENABLE_HEADING_ATTRIBUTES` set, the trailing
`{ #id .class₁ .class₂ key=val }` on an ATX heading populates the
`id: Option<CowStr>`, `classes: Vec<CowStr>`, and
`attrs: Vec<(CowStr, Option<CowStr>)>` fields on `Tag::Heading`. With
the flag unset, those fields are `None` / empty regardless of source
content (the trailer remains in the heading text).

`Heading::pretty` reads these fields out of `NodeKind::Heading` and
emits the canonical trailer (`#id` first, then classes in source
order, then `key=val` pairs in source order) when
`FmtOptions::heading_attrs` is `Canonicalise`. Under `Preserve` (the
default), the source bytes round-trip verbatim via the source-tail
read after the inline body.

Test: `heading_attributes_populate_tag_fields`.

## §10 MyST / Pandoc directives, roles, substitutions, comments

pulldown-cmark v0.13.3 emits **no** events for any of the following constructs; they are recognised entirely
through scan-and-preserve overlays in `src/ir.rs`:

| Construct                          | Scanner                  | Overlay site                                    |
| ---------------------------------- | ------------------------ | ----------------------------------------------- |
| MyST / Pandoc directive containers | `scan_directives`        | `pretty_block_sequence` (block arm)             |
| MyST `%` line comments             | `scan_comments`          | `pretty_block_sequence` (block arm)             |
| MyST inline roles                  | `scan_inline_overlays`   | `apply_inline_overlay` in `src/format/inline.rs` |
| MyST substitution references       | `scan_inline_overlays`   | `apply_inline_overlay` in `src/format/inline.rs` |
| Pandoc inline attribute spans      | `scan_inline_overlays`   | `apply_inline_overlay` in `src/format/inline.rs` |

Pulldown sees these as plain paragraph / text events, so each scanner consults the same exclusion vectors
(code blocks, inline code, HTML blocks, inline HTML; the inline scanner also excludes math regions and the
block-level directive regions) to avoid eating bytes that are already classified as something else.

A directive opener whose colon count is *n* matches the next colon-only line of count ≥ *n*. The scanner records
the outermost directive only; nested directives sit inside the outer region's bytes and are preserved implicitly.
The block overlay arm matches **by byte-range overlap** (not exact-range equality), so pulldown's
definition-list / paragraph misclassification of malformed MyST source still emits the directive bytes correctly:
when the tree node containing the bytes spans more than the directive itself, mdwright emits the union of the
tree-node range and the directive-region range verbatim.

The inline overlay (`apply_inline_overlay`) is the **first inline-level overlay** in the formatter. It splices
into both `walk_paragraph_inline` (paragraph context, with `ParagraphSafetyState`) and
`pretty_inline_children_for_ids` (emphasis / link bodies, heading inlines, list-item virtual paragraphs) before
the per-node `match`. A child whose `raw_range.start` lies inside a previously-emitted overlay region is silently
swallowed—this is the multi-child-swallow logic that handles `` {term}`payload` ``, where the role spans both
the `{term}` literal-text node and the following code-span node.

There is no drift-test for these constructs because pulldown emits nothing to drift on; the scanner's per-fixture
regression coverage in `tests/regressions/{directive_*,inline_role_*,myst_*}.in` plus the vendored
jupyter-book round-trip at `tests/external_corpora.rs` is the safety net.
