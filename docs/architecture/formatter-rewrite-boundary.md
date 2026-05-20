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

Each family sees a parsed snapshot of the current bytes. If it produces edits, the family plan checks that those edits
do not overlap within the family. A local overlap rejects the family; it does not drop one edit and keep another. If the
plan verifies, the whole plan commits and the next family sees a fresh parse. If verification fails, the family skips.

If the full family pipeline cannot reach a fixed point within the guard pass count, the formatter leaves the original
source bytes unchanged. It does not return the last verified partial output as successful formatting.

## Design comparison

| Design | Result |
| --- | --- |
| Typed candidates in one global list | Rejected. Enriching the old candidate type still leaves one shared selector that has to compare unrelated edits. It can express "keep this parent edit, drop that child edit" even when neither producer meant to own that relationship. |
| Ordered rewrite families | Chosen. Each family owns one style decision and must prove local non-overlap before commit. Cross-family order is explicit, and a family cannot silently steal ownership from another family through a range sort. |

The old global model was shallow: callers supplied a phase, owner, byte range, replacement, verification mode, and label,
then relied on a common engine to interpret those fields correctly. The family pipeline hides that coordination in the
formatter implementation. Producers no longer compete in one phase/range list.

## Ownership rules

An edit must be created for the owner kind the producer intends. There is no fallback from a requested owner to the
smallest containing owner. A list-marker edit asks for a list item; a thematic-break edit asks for a thematic break; a
math edit asks for a math region. If the matching owner does not contain the range, no edit exists.

This follows the pattern established by list marker and inline slot facts. `mdwright-document` exposes marker-local
facts, delimiter slots, and link destination slots, so nested constructs cannot be represented as one enclosing rewrite
that accidentally covers child bytes.

## Remaining hardening

This boundary removes the global selector and partial-success failure mode. Later passes should deepen the remaining
parent and terminal families:

- table padding should compute a parent table normal form after child canonicalisers run;
- wrap should remain terminal and skip unsupported paragraph shapes rather than racing inline canonicalisation.
