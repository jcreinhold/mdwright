# Formatter rewrite boundary

The formatter starts from identity emit. Opt-in style and wrap changes run through private rewrite families in
`mdwright-format`; each family builds a locally non-overlapping plan, verifies the resulting document, and commits the
whole plan or none of it. Verification is a safety gate. It is not a convergence strategy.

Parser facts stay in `mdwright-document`. Rewrite policy stays in `mdwright-format`. The document crate tells the
formatter where the syntactic slots are; the formatter decides whether a configured style should rewrite those slots.

## Adopted design

The rewrite subsystem uses ordered families:

1. inline delimiters;
2. list markers;
3. thematic breaks;
4. link destinations;
5. heading attributes;
6. table normal forms;
7. math;
8. frontmatter;
9. terminal wrap.

Each canonical family sees a parsed snapshot of the current bytes. If it produces edits, the family plan checks that
those edits do not overlap within the family. A local overlap rejects the family; it does not drop one edit and keep
another. If the plan verifies, the whole plan commits and the pipeline starts again from the first canonical family on a
fresh parse. If verification fails, the family skips; verification never repairs an incomplete plan.

Terminal wrap is not a peer canonical family. It runs only after a full canonical-family scan commits nothing for the
current snapshot. If wrap commits paragraph edits, the pipeline starts again from the first canonical family so any
newly exposed syntactic slots are normalized before wrap runs again.

The successful terminal state is a full pass with no family commits. If the guard pass count trips before that state,
the formatter leaves the original source bytes unchanged. It does not return the last verified partial output as
successful formatting.

## Design comparison

| Design | Result |
| --- | --- |
| Typed candidates in one global list | Rejected. Enriching the old candidate type still leaves one shared selector that has to compare unrelated edits. It can express "keep this parent edit, drop that child edit" even when neither producer meant to own that relationship. |
| Ordered rewrite families | Chosen. Each family owns one style decision and must prove local non-overlap before commit. Cross-family order is explicit, and a family cannot silently steal ownership from another family through a range sort. |

The old global model was shallow: callers supplied a phase, owner, byte range, replacement, verification mode, and
label, then relied on a common engine to interpret those fields correctly. The family pipeline hides that coordination
in the formatter implementation. Producers no longer compete in one phase/range list.

## Ownership rules

An edit must be created for the owner kind the producer intends. There is no fallback from a requested owner to the
smallest containing owner. A list-marker edit asks for a list item; a thematic-break edit asks for a thematic break; a
math edit asks for a math region. If the matching owner does not contain the range, no edit exists.

This follows the pattern established by list marker and inline slot facts. `mdwright-document` exposes marker-local
facts, delimiter slots, and link destination slots, so nested constructs cannot be represented as one enclosing rewrite
that accidentally covers child bytes.

## Table Normal Forms

Table padding is a parent operation. It runs after inline delimiter and link-destination families, reads cell bytes from
the current snapshot, and rewrites the whole table block as one verified operation. Row and cell edits are not exposed
as candidates.

The table family uses document-owned table facts: source ranges for the table, rows, cells, and alignments. If a row has
source cells beyond the recognised table column count, or a cell range is not contained in its row, the table family
skips that table instead of dropping bytes it cannot model.

## Terminal Wrap

Paragraph wrapping is a terminal operation. It reads document-owned paragraph facts from the current snapshot: line
ranges, content ranges, prefixes, hard breaks, and inline atomics. It computes paragraph replacements after all earlier
canonicalizers have reached local normal form, verifies the paragraph batch, and commits the batch or none of it.

Unsupported paragraph shapes stay unchanged. They are counted in the formatter report rather than widened into paragraph
edits whose safety depends on later passes.
