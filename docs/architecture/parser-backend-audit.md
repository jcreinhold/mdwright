# Parser Backend Audit

`cargo xtask parser-audit` compares mdwright's production `pulldown-cmark` backend with `cmark-gfm`, using the
vendored GFM spec expected HTML as the primary oracle. The audit renders mdwright through the opt-in `cmark-gfm` render
profile so renderer spelling drift is separated from parser-tree drift. It does not replace `mdwright-document` as the
production parser boundary.

`cmark-gfm` is the primary oracle because `crates/mdwright/tests/gfm-spec/spec.txt` is vendored from cmark-gfm and the
GFM ecosystem treats its rendered HTML as the reference. `comrak` is optional diagnostic evidence for rendered HTML and
source-position behaviour; it is not a release gate unless a future audit shows it catches mdwright-relevant risks that
cmark-gfm cannot expose.

## Running

```sh
cargo xtask parser-audit \
  --case-set all \
  --output target/mdwright/parser-audit \
  --ensure-tools \
  --include-comrak
```

The command builds a pinned `cmark-gfm` under `target/mdwright/tools/` when `--ensure-tools` is passed. To use an
already-built binary explicitly, pass `--cmark-gfm-bin <path>`.

Reports are written to:

- `target/mdwright/parser-audit/parser-audit.json`
- `target/mdwright/parser-audit/parser-audit.md`

Examples marked `disabled` in the vendored GFM spec are still reported, but cmark-gfm binary drift from the expected
HTML for those cases is not a command failure because the upstream spec does not treat the rendered checkbox spelling as
a strict conformance assertion.

The audit also checks source-position envelopes for constructs mdwright uses as formatter/linter facts. It maps
cmark-gfm `data-sourcepos` line/column ranges back to source bytes and compares them against mdwright document facts
by construct kind. This is a risk gate, not exact AST equality: a difference is reported only when mdwright has no
overlapping fact for a rewrite/lint-owned construct.

## Status Values

- `pulldown-html-mismatch`: mdwright's pulldown-backed HTML differs from cmark-gfm expected HTML.
- `mdwright-policy`: mdwright intentionally differs from the cmark-gfm oracle for a documented parser policy.
- `extension-gap`: the compared parser does not implement the construct.
- `sourcepos-risk`: rendered output matches, but coordinate facts may affect formatter/lint safety.
- `event-only`: internal event/AST shape differs while rendered HTML and semantic signatures match.
- `upstream-panic`: parser panic or crash contained by `mdwright-document`.
- `needs-mdwright-mitigation`: upstream behaviour is unsafe for mdwright and still needs a fix.
- `fixed`: the difference should no longer appear; the audit fails if it does.

## Classifications

Current `gfm-spec` audit snapshot with mdwright's `cmark-gfm` render profile:

| Metric | Count |
| --- | ---: |
| Cases | 673 |
| HTML mismatches | 15 |
| Sourcepos envelopes checked | 1071 |
| Sourcepos differences | 0 |
| Unclassified differences | 0 |

Observed difference classes:

| Observed | Count |
| --- | ---: |
| `pulldown-html-mismatch:emphasis-resolution` | 9 |
| `pulldown-html-mismatch:html-block-rendering` | 3 |
| `pulldown-html-mismatch:tasklist-rendering` | 2 |
| `pulldown-html-mismatch:table-rendering` | 1 |
| `upstream-panic` | 1 |

| Case Set | Key | Observed | Status | Owner | Resolution |
| --- | --- | --- | --- | --- | --- |
| * | * | mdwright-policy:gfm-bare-autolinks-enabled | fixed | document | Parser-audit now mirrors the cmark-gfm extension set per spec case, so default production GFM policy no longer creates non-extension CommonMark audit drift. |
| * | * | mdwright-policy:gfm-email-autolinks-disabled | fixed | document | GFM email autolinks are recognised by mdwright-document's source-positioned GFM overlay. |
| * | * | mdwright-policy:gfm-tagfilter-disabled | fixed | document | GFM tagfiltering is enabled by default in mdwright-document's render/signature policy. |
| * | * | pulldown-html-mismatch:gfm-autolink | fixed | document | GFM URL and email autolink mismatches should be handled by mdwright-document's GFM autolink overlay. |
| * | * | pulldown-html-mismatch:gfm-tagfilter | fixed | document | GFM tagfilter mismatches should be handled by mdwright-document's GFM tagfilter overlay. |
| * | * | pulldown-html-mismatch:quote-escaping | fixed | document | The `cmark-gfm` render profile escapes double quotes in text/code contexts where cmark-gfm emits `&quot;`. |
| * | * | pulldown-html-mismatch:href-escaping | fixed | document | The `cmark-gfm` render profile percent-encodes link destinations where cmark-gfm percent-encodes them. |
| gfm-spec | Tables (extension) | pulldown-html-mismatch:table-rendering | fixed | document | The `cmark-gfm` render profile spells ordinary GFM table markup with cmark-gfm row, alignment, and body layout. |
| gfm-spec | case-160 | pulldown-html-mismatch:table-rendering | pulldown-html-mismatch | document | This is a raw HTML table containing indented code, not a GFM table. The remaining drift is parser/backend handling of blank raw-HTML text around child blocks, not formatter rewrite risk. |
| gfm-spec | case-279, case-280 | pulldown-html-mismatch:tasklist-rendering | pulldown-html-mismatch | document | These spec examples are marked `disabled`; cmark-gfm's binary output and mdwright's `cmark-gfm` profile match, while the vendored expected HTML intentionally does not assert the checkbox spelling. |
| * | * | extension-gap:myst-definition-list | extension-gap | document | cmark-gfm does not own MyST directive syntax; mdwright's default definition-list recognition can make directive-heavy fixtures render differently through pulldown HTML, while formatter preservation is handled by mdwright document facts. |
| corpus | external:jupyter_book_minimal/admonitions.md, external:jupyter_book_minimal/asides.md, external:jupyter_book_minimal/blocks.md, external:jupyter_book_minimal/directives.md | sourcepos-risk:paragraph | extension-gap | document | cmark-gfm reports MyST directive/admonition syntax as ordinary paragraph source ranges, while mdwright treats the same bytes as extension-owned containers or preservation facts. The corpus rows pin that non-GFM coordinate drift so it cannot silently expand. |
| gfm-spec | case-120, case-152, case-153 | pulldown-html-mismatch:html-block-rendering | pulldown-html-mismatch | document | pulldown's event stream omits leading indentation on raw HTML blocks that cmark-gfm preserves in rendered HTML. mdwright accepts this as backend render drift because source-coordinate facts remain stable. |
| gfm-spec | case-144 | pulldown-html-mismatch:html-block-rendering | fixed | document | The `cmark-gfm` render profile now matches cmark-gfm's newline placement for this list/raw-HTML case. |
| gfm-spec | case-398, case-426, case-434, case-435, case-436, case-473, case-474, case-475, case-477 | pulldown-html-mismatch:emphasis-resolution | pulldown-html-mismatch | document | pulldown's emphasis resolution differs from cmark-gfm on these delimiter-stack edge cases; mdwright currently treats this as a parser-backend conformance gap, not a formatter-local bug. |
| operational | known-pulldown-link-ref-tab-panic | upstream-panic | upstream-panic | document | pulldown-cmark issue 1095 is contained by `mdwright-document::ParseError`; product paths do not panic. |

The `cmark-gfm` render profile is an HTML spelling profile. It fixes quote escaping, link-destination escaping, ordinary
GFM table spelling, task-list checkbox spelling, and one newline-placement case where the parser already exposes enough
structure. It does not change emphasis resolution or source-position semantics. Full cmark-gfm parser equivalence would
require upstream pulldown changes, a maintained fork, or a backend switch.

## Replacement Criteria

Do not replace `pulldown-cmark` based on event-shape differences alone. A replacement candidate must improve at least
one release-relevant axis without regressing the others:

- fewer unclassified or policy-relevant HTML mismatches against cmark-gfm;
- safer behaviour on malformed/user input;
- stable byte/source coordinates sufficient for formatter rewrite ownership;
- extension coverage at least as good as the current document facts;
- acceptable runtime and dependency footprint.
