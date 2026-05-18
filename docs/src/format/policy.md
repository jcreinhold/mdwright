# Formatter policy

mdwright's formatter has two responsibilities, in this order:

## 1. Structural emit — preserve

Take the parsed IR back to bytes that re-parse to the same IR. This pass is **pure
preservation**: every emit site picks the source's own representation, so the output is
byte-equivalent to the source wherever the source was already valid. Emphasis delimiters,
list bullets, thematic-break characters, link-destination angle brackets, ordered-list
numbering — all of them survive unchanged.

This is the load-bearing invariant. It is idempotent by construction (you cannot perturb
a representation that you copy verbatim), and any input that round-trips through pulldown's
parser round-trips through the formatter.

You opt out of preservation by setting the second-pass knobs (below). There is no
"semi-preserve" mode.

## 2. Style canonicalisation — opt-in

A separate post-structural pass at `src/format/canonicalise.rs` rewrites the structural
output per the style knobs in `[fmt]`. Each rewrite is **verified locally**: rewrite a
byte sequence, reparse the affected paragraph window, confirm the canonical event stream
is unchanged. If verification fails, the rewrite is skipped (the source-preserved bytes
stay) and a `tracing::warn!` records the skip.

The pass iterates internally to a fixed point (capped at 8 iterations), then returns.

Default: every knob is `Preserve`. With the default config the pass short-circuits before
running. Set per-knob targets in `.mdwright.toml` to opt in.

## Why the separation

Earlier mdwright designs canonicalised during structural emit. The result was a bug class
that proved hard to extinguish: emit decisions perturbed their own context. Rewriting
`_foo_` to `*foo*` changed the adjacent site's emphasis-flanking class, which could
cascade. The fix at the time was a convergence loop wrapped around the whole formatter,
plus a safety ladder of escape-and-reparse checks at each emit site (~800 lines of
recovery machinery).

With structural emit pure-preserving, the perturbation source is gone. With
canonicalisation isolated to one verified pass, failed rewrites are localised rather than
global. The convergence loop and safety ladder were deleted in the v0.4.0 redesign.

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
- Does not consult adjacent paragraphs when verifying a rewrite. The verification window
  is one paragraph (from the previous blank line through the next blank line). Rewrites
  whose effect spans a paragraph boundary are conservative; in practice this is only
  visible for thematic-break runs and is handled correctly by the chokepoint parse.

For aggressive cross-knob canonicalisation as a default (with the more invasive
representation choices that follow from it),
[mdformat](https://mdformat.readthedocs.io/) is a good alternative.
