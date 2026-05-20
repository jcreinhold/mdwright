# Formatter policy

mdwright's formatter has two responsibilities, in this order:

## 1. Identity Emit: Preserve

Start with the user's source bytes. With every style knob at its default and `wrap = "keep"`, formatting returns those
bytes unchanged except for the document-boundary policies: line endings, trailing newline handling, and end-of-line
selection.

This is the load-bearing invariant. Default formatting is idempotent by construction because the formatter does not
synthesise Markdown for recognised structures.

You opt out of preservation by setting the rewrite knobs below. There is no "semi-preserve" mode.

## 2. Verified Rewrite Families: Opt In

The formatter crate runs style-canonicalisation and wrapping through private rewrite families: inline delimiters, list
markers, thematic breaks, link destinations, heading attributes, tables, math, frontmatter, and terminal wrap. Each
canonical family builds a local normal-form edit plan, proves its edits do not overlap within the family, applies the
plan to a scratch buffer, and verifies the result before it can commit.

If verification fails, the whole family skips. The engine never commits half of a family plan. If the family pipeline
cannot reach a pass with no commits before its guard trips, mdwright leaves the original source bytes unchanged instead
of returning a partial normal form.

Tables are parent normal forms. The table family runs after inline canonicalisers, reads cell contents from the current
snapshot, and rewrites each table block only when document-owned table facts account for the full table shape. It does
not emit row- or cell-level edits that could race inline rewrites.

Wrap is terminal. It runs only after a full canonical-family scan commits no edits for the current snapshot. If wrap
commits paragraph edits, the engine returns to the first canonical family on a fresh parse before wrapping again.
Paragraph shapes the wrap pass cannot model stay unchanged and are counted in the formatter report.

An integer wrap setting is a line-budget contract, not a profile-specific preference. With `wrap = 120`, breakable
paragraph lines are kept at or below 120 display columns in both the default formatter profile and the mdformat profile.
The only accepted overflow is one indivisible atomic token, such as a code span, URL, math atom, or single long word.
The default wrap strategy is stable soft-break reflow: ordinary source newlines inside a paragraph may be joined, hard
breaks stay hard boundaries, and overlong breakable runs are wrapped to the configured budget. `wrap-strategy =
"balanced"` opts into a paragraph rebalancer for authors who prefer more even line lengths.

Default: every style knob is `Preserve` and wrapping is `Keep`. With the default config the rewrite-family pipeline
short-circuits before running. Set per-knob targets in `.mdwright.toml` to opt in.

## Why the separation

Earlier mdwright designs canonicalised while synthesising structural output. The result was a bug class where one emit
decision perturbed the context for another. Rewriting `_foo_` to `*foo*`, for example, can change an adjacent site's
emphasis-flanking class.

Identity emit removes that perturbation source. Rewrite families keep the remaining byte changes in formatter-owned
normal-form plans, so stale local string edits cannot commit without reparsing and verification.

## How to opt in

In `.mdwright.toml`:

```toml
[fmt]
italic = "asterisk"            # _foo_ → *foo*
strong = "underscore"          # **bar** → __bar__
list-marker = "dash"           # * x   → - x
thematic-break = "dash"        # *** → ---
ordered-list = "consistent"    # 3. a / 5. b / 9. c → 3. a / 4. b / 5. c

[fmt.refs]
style = "angle"                # [ref]: url → [ref]: <url>
```

Each knob also accepts `"preserve"` to explicitly disable canonicalisation. See [Style knobs](style.md) for the per-knob
reference, including which rewrites might skip verification (e.g. intraword underscore that can't safely become
asterisk).

## What the canonicalisation pass does NOT do

- Does not rewrap prose (`wrap` is a separate knob; see [Configuration](../configuration.md)).
- Does not change content semantics: every rewrite must reparse to the same canonical event stream as the bytes it
  replaces, or it is skipped.
- Does not expose rewrite families, snapshot ownership, or verification signatures as public API. Those details stay
  private to `mdwright-format`.

For mdformat-compatible spelling where verified rewrites preserve the parsed document, use `[fmt] profile = "mdformat"`.
