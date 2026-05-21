# Pulldown-cmark model

Reference for the per-construct behaviours of `pulldown-cmark` 0.13 that mdwright depends on. Every emit-site decision
in `crates/mdwright-format` either matches a rule on this page or contradicts pulldown. A contradiction is a bug.

This file is paired with `crates/mdwright/tests/pulldown_model.rs`. Each rule below has one test in that file that feeds
the documented example to pulldown and asserts the documented event-stream shape. When pulldown changes upstream (a
release bump, a bug fix on their side), the test fails and **this document must be updated before any mdwright code is
changed in response**.

Every production parse flows through private helpers in `crates/mdwright-document/src/parse.rs`, which take a private
`CanonicalSource<'_>`. Construction routes through the document crate's source canonicalisation, so pulldown's input is
*always* CR-free and NUL-free in production. Rules below assume that pre-condition.

## §1 Line endings

`Source::canonicalise` strips CR / CRLF → LF and NUL → U+FFFD before pulldown sees the buffer (CM §2.1, §2.3). Inside
HTML blocks, code blocks, math regions, and inline code, pulldown preserves the (now-LF) bytes verbatim in the `CowStr`
payload. In prose, a single `\n` between non-blank content lines becomes `Event::SoftBreak`; two consecutive `\n`s end
the current block.

Consequence: no `CowStr` produced by `Event::Text`, `Event::Code`, `Event::Html`, `Event::InlineHtml`,
`Event::InlineMath`, or `Event::DisplayMath` can ever contain a CR byte in production. The semantic-equivalence walker
in `crates/mdwright-format` relies on this; there is no per-event CR scrub.

Test: `line_endings_softbreak_between_lines`.

## §2 Trailing blank lines in containers

Pulldown strips trailing blank lines from indented code blocks before emitting the final `Event::Text`. A
whitespace-only line is "blank."

The source `"\t|\n\t"` produces a single `Event::Text("|\n")` inside the indented code block: the trailing tab-only line
is consumed as a blank line, but the terminating `\n` of the *content* line stays in the payload. The formatter's
`normalize_trailing_newline` consumes that trailing LF when re-emitting; without it the formatter would emit one
trailing LF too many.

Cite: regression fixture `crates/mdwright/tests/regressions/fuzz_indented_code_trailing_ws_drop.in`.

Test: `indented_code_keeps_content_terminating_newline`.

## §3 Emphasis pairing scope

CM §6.2 / §6.3: emphasis delimiters pair *within their enclosing pairing container*. The set of pairing containers
pulldown observes: paragraph, heading, table cell, link body, image body, footnote definition.

Strikethrough (`~~…~~`) is **not** a pairing container: emphasis delimiters can open inside one strikethrough run and
close inside another, or across a strikethrough boundary entirely. The canonicalisation pass's per-rewrite verification
window includes surrounding bytes so a candidate that would re-pair across a strikethrough boundary is rejected.

Link bodies *are* a pairing boundary because CM §6.5 gives link text grouping higher precedence than emphasis grouping.
The two are not symmetric: `*[foo*](bar)` parses with the `*` not pairing (it's outside the link, the link doesn't
enclose it), but the link text `[foo*]` does not contribute to an outer `*…*` pair either.

Test: `emphasis_pairs_within_paragraph` and `emphasis_pairs_across_strikethrough` and
`link_body_breaks_emphasis_pairing`.

## §4 Reference label normalisation

CM §4.7: trim leading and trailing whitespace; collapse internal runs of whitespace to a single U+0020; case-fold via
Unicode default case folding. Two labels resolve to the same definition iff their normalised forms agree.

Pulldown 0.13 does **not** emit a `LinkReferenceDefinition` event. Definitions are resolved internally during parse, and
reference uses surface as `Tag::Link { id: ".." }` where `id` is the *raw label bytes* the source used (not the
normalised form). The mdwright-side authoritative scan for definitions lives in
`crates/mdwright-document/src/refs.rs::build_reference_table`; that module is the sole site that runs CM §4.7
normalisation.

Test: `reference_label_normalisation_matches`.

## §5 HTML block boundaries

CM §4.6 defines seven HTML block types, each with its own start / end conditions. Two of the important asymmetries:

- **Type 2** (`<!-- … -->` or `<?…?>` style with a multi-char end marker): the block ends at the *line containing* the
  matching end marker (or EOF). The block's events are a sequence of `Event::Html(line)` per source line, each payload
  including its trailing newline, except possibly the last, which can omit the newline if the source did.
- **Type 6** (recognised tag names like `<table>`): the block ends at the first blank line after the start (or EOF).
  Recognition is by tag *name*, not by close-tag matching: `<table>` opens a type-6 block; the close `</table>` does not
  by itself end it. A blank line does.

The block's payload bytes round-trip verbatim (modulo §1 canonicalisation), so the formatter emits HTML blocks by
stamping the captured source slice rather than reconstructing from events.

Test: `html_block_type2_emits_per_line_events`.

## §6 Emphasis-event range semantics

`Event::Start(Tag::Emphasis)` and `Event::End(TagEnd::Emphasis)` ranges in the offset iterator cover the **entire run**,
from the byte position of the first character of the opening delimiter, to the byte position *after* the last character
of the closing delimiter.

- `range.start` of `Start(Emphasis)`: index of the first `*` or `_` of the opening run.
- `range.end` of `End(Emphasis)`: index *after* the last `*` or `_` of the closing run.
- The body bytes occupy `[start_range.end, end_range.start)`.

Same convention for `Strong`. `mdwright-document` turns these ranges into inline delimiter-slot facts that name only the
opening and closing delimiter bytes. A pulldown change to either range convention would silently change those facts; the
model test catches the drift first.

Test: `emphasis_event_range_spans_delimiters`.

## §7 Strong vs nested emphasis disambiguation

CM §6.5 disambiguates runs of two through six `*` / `_` characters:

- `**foo**` → `Start(Strong)`, `Text("foo")`, `End(Strong)`. Not emphasis-of-emphasis.
- `***foo***` → `Start(Strong)`, `Start(Emphasis)`, `Text("foo")`, `End(Emphasis)`, `End(Strong)` (the nesting order
  depends on pairing direction; pulldown's left-flank rule decides).
- `*_foo_*` → `Start(Emphasis)`, `Start(Emphasis)`, `Text("foo")`, `End(Emphasis)`, `End(Emphasis)`. Two distinct
  delimiter characters pair independently.

Canonicalisation must keep these distinct. Inline delimiter families edit only delimiter slots and verify the resulting
document before commit; a rewrite that would let pulldown re-segment the construct differently is skipped.

Test: `strong_distinct_from_nested_emphasis`.

## §8 Definition-list event shape

With `Options::ENABLE_DEFINITION_LIST` set on the parser, the source

```
Term
: defn
```

emits the nested triple `Start(DefinitionList)` → `Start(DefinitionListTitle)` → … → `End(DefinitionListTitle)` →
`Start(DefinitionListDefinition)` → … → `End(DefinitionListDefinition)` → `End(DefinitionList)`. Each definition's body
is opened/closed independently, so a definition containing multiple paragraphs emits multiple `Start(Paragraph)` /
`End(Paragraph)` pairs inside one `DefinitionListDefinition`.

The private document tree relies on this nesting shape to construct definition-list nodes in
`crates/mdwright-document/src/tree.rs`. Public callers consume document facts and signatures; they do not see pulldown's
event nesting directly.

Test: `definition_list_emits_tag_triple`.

## §9 Heading attribute fields

With `Options::ENABLE_HEADING_ATTRIBUTES` set, the trailing `{ #id .class₁ .class₂ key=val }` on an ATX heading
populates the `id: Option<CowStr>`, `classes: Vec<CowStr>`, and `attrs: Vec<(CowStr, Option<CowStr>)>` fields on
`Tag::Heading`. With the flag unset, those fields are `None` / empty regardless of source content (the trailer remains
in the heading text).

`mdwright-document` records the parsed trailer as a `HeadingAttrSite`. The `mdwright-format` heading-attribute family
emits the canonical trailer (`#id` first, then classes in source order, then `key=val` pairs in source order) when
`FmtOptions::heading_attrs` is `Canonicalise`. Under `Preserve` (the default), the source bytes round-trip unchanged.

Test: `heading_attributes_populate_tag_fields`.

## §10 MyST / Pandoc directives, roles, substitutions, comments

pulldown-cmark v0.13.3 emits **no** events for any of the following constructs; mdwright treats them as source-owned
extension regions under document parse policy:

| Construct                          | Owning policy                                  |
| ---------------------------------- | ---------------------------------------------- |
| MyST / Pandoc directive containers | `ParseOptions::extensions.myst.directive_containers` |
| MyST `%` line comments             | `ParseOptions::extensions.myst.comments`       |
| MyST inline roles                  | `ParseOptions::extensions.myst.inline_roles`   |
| MyST substitution references       | `ParseOptions::extensions.myst.substitution_references` |
| Pandoc inline attribute spans      | `ParseOptions::extensions.pandoc.inline_attribute_spans` |

Pulldown sees these as plain paragraph / text events. mdwright therefore treats their source bytes as opaque unless a
document-owned fact proves a narrower rewrite slot nearby.

For directive containers, an opener whose colon count is *n* matches the next colon-only line of count ≥ *n*. Nested
directive bytes are preserved by source identity.

The formatter starts from source bytes, so unknown extension syntax is preserved by default. Opt-in rewrite families
must use document-owned facts and exclusion regions before touching bytes near these constructs.

There is no drift test for these constructs because pulldown emits nothing to drift on. Per-fixture regression coverage
in `crates/mdwright/tests/regressions/{directive_*,inline_role_*,myst_*}.in` plus the vendored jupyter-book round trip
at `crates/mdwright/tests/external_corpora.rs` is the safety net.
