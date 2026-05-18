# Formatter policy

mdwright's formatter has two responsibilities, in this order:

## 1. Identity emit — preserve

Start with the user's source bytes. With every style knob at its default and `wrap = "keep"`, formatting returns those
bytes unchanged except for the document-boundary policies: line endings, trailing newline handling, and end-of-line
selection.

This is the load-bearing invariant. Default formatting is idempotent by construction because the formatter does not
synthesise Markdown for recognised structures.

You opt out of preservation by setting the rewrite knobs below. There is no "semi-preserve" mode.

## 2. Verified rewrites — opt-in

The formatter crate collects style-canonicalisation and wrapping candidates, then applies them through one private
transactional rewrite engine. Each candidate is owned by the current parsed document snapshot, ordered deterministically,
checked for overlap, applied to a scratch buffer, and verified before it can commit.

If verification fails, the engine retries candidates individually and commits only the ones that still preserve the
document and math signatures. The engine iterates to a fixed point, capped at 8 iterations.

Default: every style knob is `Preserve` and wrapping is `Keep`. With the default config the rewrite engine short-circuits
before running. Set per-knob targets in `.mdwright.toml` to opt in.

## Why the separation

Earlier mdwright designs canonicalised while synthesising structural output. The result was a bug class where one emit
decision perturbed the context for another. Rewriting `_foo_` to `*foo*`, for example, can change an adjacent site's
emphasis-flanking class.

Identity emit removes that perturbation source. The transactional rewrite engine keeps the remaining byte changes in
one formatter-owned abstraction, so stale local string edits cannot commit without reparsing and verification.

## How to opt in

In `.mdwright.toml`:

```toml
[fmt]
italic = "asterisk"            # _foo_ → *foo*
strong = "underscore"          # **bar** → __bar__
list-marker = "dash"           # * x   → - x
thematic-break = "dash"        # *** → ---
ordered-list = "consistent"    # 3. a / 5. b / 9. c → 1. a / 2. b / 3. c

[fmt.refs]
style = "angle"                # [ref]: url → [ref]: <url>
```

Each knob also accepts `"preserve"` to explicitly disable canonicalisation. See
[Style knobs](style.md) for the per-knob reference, including which rewrites might skip
verification (e.g. intraword underscore that can't safely become asterisk).

## What the canonicalisation pass does NOT do

- Does not rewrap prose (`wrap` is a separate knob; see [Configuration](../configuration.md)).
- Does not change content semantics — every rewrite must reparse to the same canonical
  event stream as the bytes it replaces, or it is skipped.
- Does not expose rewrite candidates, snapshot ownership, or verification signatures as public API. Those details stay
  private to `mdwright-format`.

For aggressive cross-knob canonicalisation as a default (with the more invasive
representation choices that follow from it),
[mdformat](https://mdformat.readthedocs.io/) is a good alternative.
