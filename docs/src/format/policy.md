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

The formatter crate runs style-canonicalisation and wrapping through one private transactional rewrite engine. The
engine is organized as ordered rewrite families: inline delimiters, list markers, thematic breaks, link destinations,
heading attributes, tables, math, frontmatter, and terminal wrap. Each family builds a local edit plan, proves its edits
do not overlap within the family, applies the plan to a scratch buffer, and verifies the result before it can commit.

If verification fails, the whole family skips. The engine never commits half of a family plan. If the family pipeline
cannot reach a fixed point within its guard pass count, mdwright leaves the original source bytes unchanged instead of
returning a partial normal form.

Tables are parent normal forms. The table family runs after inline canonicalisers, reads cell contents from the current
snapshot, and rewrites each table block only when document-owned table facts account for the full table shape. It does
not emit row- or cell-level edits that could race inline rewrites.

Default: every style knob is `Preserve` and wrapping is `Keep`. With the default config the rewrite engine
short-circuits before running. Set per-knob targets in `.mdwright.toml` to opt in.

## Why the separation

Earlier mdwright designs canonicalised while synthesising structural output. The result was a bug class where one emit
decision perturbed the context for another. Rewriting `_foo_` to `*foo*`, for example, can change an adjacent site's
emphasis-flanking class.

Identity emit removes that perturbation source. The transactional rewrite engine keeps the remaining byte changes in
formatter-owned rewrite families, so stale local string edits cannot commit without reparsing and verification.

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

Each knob also accepts `"preserve"` to explicitly disable canonicalisation. See [Style knobs](style.md) for the
per-knob reference, including which rewrites might skip verification (e.g. intraword underscore that can't safely become
asterisk).

## What the canonicalisation pass does NOT do

- Does not rewrap prose (`wrap` is a separate knob; see [Configuration](../configuration.md)).
- Does not change content semantics: every rewrite must reparse to the same canonical event stream as the bytes it
  replaces, or it is skipped.
- Does not expose rewrite families, snapshot ownership, or verification signatures as public API. Those details stay
  private to `mdwright-format`.

For mdformat-compatible spelling where verified rewrites preserve the parsed document, use `[fmt] profile = "mdformat"`.
