# mdwright 0.3.0 — the spec-alignment redesign

**2026-05-16.** mdwright 0.3.0 replaces the formatter's per-byte sieve with a typed intermediate representation. Each
CommonMark/GFM construct is now a Rust value whose constructor enforces the spec's well-formedness rules, and each value
has its own `pretty()` method. There is no longer a single "format pass" that walks the tree applying rewrites; there is
the IR, and the IR knows how to print itself.

## What this means for users

- The `fmt` and `fmt --check` subcommands behave the same. Defaults are unchanged. Configuration files written against
  0.1.0 continue to work.
- A new `--mode=verbatim` flag bypasses the typed IR and emits source bytes 1-to-1. Use it on documents where the
  default normalising mode diverges from what you want to preserve.
- A new `-v` / `-vv` / `-vvv` count flag controls structured logging (silent by default). At `-vvv` you see
  per-construct decisions, which is useful when debugging why a particular run-trip is not idempotent.

## What this means for the codebase

- Spec conformance is now a *construction-time* property of each IR value rather than a 672-case runtime sieve. Bugs
  that previously hid inside the sieve's accumulated state surface as constructor-precondition failures.
- The GFM 0.29-gfm spec round-trips 605 of 672 examples; the remaining 67 are tracked in `tests/gfm-spec/snapshot.txt`
  and summarised in [`docs/deviations.md`](../deviations.md). The editorial-deviation allowlist is empty at launch — we
  are deliberately conservative about declaring a divergence permanent.
- Per-construct round-trip property tests replace most of the whole-document sieve runner.

## Performance

The steady-state format step (parse outside the timed region) is **25–27 % faster** than the v0.2.0 sieve on
micro-benches:

| Bench              | v0.2.0  | v0.3.0  | Δ       |
| ------------------ | ------- | ------- | ------- |
| `format/small`     | 0.216ms | 0.159ms | −26.5 % |
| `format/medium`    | 0.368ms | 0.271ms | −26.5 % |
| `format_wrap/keep` | 0.368ms | 0.268ms | −27.2 % |

The end-to-end `parse_plus_format` path is 8–15 % slower because IR construction now does more work per pulldown-cmark
event. The `mdwright fmt --check docs/` wall-clock metric (still ~128 ms on the project corpus) is dominated by parallel
I/O and parse, so the regression is not visible at that level. A follow-up release will close the parse-side gap.

## Lines of code

The redesign added ~5 k lines net: the typed IR modules (`mdwright::cm::{inline, block, refs}`), per-construct
round-trip proptests, and `docs/deviations.md` together more than offset the ~3 k lines deleted from the sieve. This is
the explicit trade we wanted: more code, but each piece is *local* — the constructor for a typed value is the only thing
that decides how that value round-trips, and the proptest for that constructor is the only thing that needs to change
when the rule changes.

## Where to read next

- Full CHANGELOG: [`CHANGELOG.md`](../../CHANGELOG.md)
- Spec conformance index: [`docs/deviations.md`](../deviations.md)
- Snapshot mechanism: [`tests/gfm-spec/README.md`](../../tests/gfm-spec/README.md)
