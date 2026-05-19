# Parser Backend Audit

`cargo xtask parser-audit` compares mdwright's production `pulldown-cmark` backend with `cmark-gfm`, using the vendored
GFM spec expected HTML as the primary oracle. The audit characterises parser-backend differences; it does not replace
`mdwright-document` as the production parser boundary.

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
HTML for those cases is not a command failure because the upstream spec does not treat the rendered checkbox spelling
as a strict conformance assertion.

The audit also checks source-position envelopes for constructs mdwright uses as formatter/linter facts. It maps
cmark-gfm `data-sourcepos` line/column ranges back to source bytes and compares them against mdwright document facts by
construct kind. This is a risk gate, not exact AST equality: a difference is reported only when mdwright has no
overlapping fact for a rewrite/lint-owned construct.

## Status Values

- `pulldown-html-mismatch`: pulldown-rendered HTML differs from cmark-gfm expected HTML.
- `mdwright-policy`: mdwright intentionally differs from the cmark-gfm oracle for a documented parser policy.
- `extension-gap`: the compared parser does not implement the construct.
- `sourcepos-risk`: rendered output matches, but coordinate facts may affect formatter/lint safety.
- `event-only`: internal event/AST shape differs while rendered HTML and semantic signatures match.
- `upstream-panic`: parser panic or crash contained by `mdwright-document`.
- `needs-mdwright-mitigation`: upstream behaviour is unsafe for mdwright and still needs a fix.
- `fixed`: the difference should no longer appear; the audit fails if it does.

## Classifications

Current `gfm-spec` audit snapshot after GFM autolink and tagfilter support:

| Metric | Count |
| --- | ---: |
| Cases | 673 |
| Pulldown HTML mismatches | 40 |
| Sourcepos envelopes checked | 1071 |
| Sourcepos differences | 0 |
| Unclassified differences | 0 |

Observed difference classes:

| Observed | Count |
| --- | ---: |
| `pulldown-html-mismatch:quote-escaping` | 21 |
| `pulldown-html-mismatch:emphasis-resolution` | 9 |
| `pulldown-html-mismatch:table-rendering` | 7 |
| `pulldown-html-mismatch:tasklist-rendering` | 2 |
| `pulldown-html-mismatch:html-block-rendering` | 1 |
| `upstream-panic` | 1 |

| Case Set | Key | Observed | Status | Owner | Resolution |
| --- | --- | --- | --- | --- | --- |
| * | * | mdwright-policy:gfm-bare-autolinks-enabled | fixed | document | Parser-audit now mirrors the cmark-gfm extension set per spec case, so default production GFM policy no longer creates non-extension CommonMark audit drift. |
| * | * | mdwright-policy:gfm-email-autolinks-disabled | fixed | document | GFM email autolinks are recognised by mdwright-document's source-positioned GFM overlay. |
| * | * | mdwright-policy:gfm-tagfilter-disabled | fixed | document | GFM tagfiltering is enabled by default in mdwright-document's render/signature policy. |
| * | * | pulldown-html-mismatch:gfm-autolink | fixed | document | GFM URL and email autolink mismatches should be handled by mdwright-document's GFM autolink overlay. |
| * | * | pulldown-html-mismatch:gfm-tagfilter | fixed | document | GFM tagfilter mismatches should be handled by mdwright-document's GFM tagfilter overlay. |
| * | * | pulldown-html-mismatch:quote-escaping | pulldown-html-mismatch | document | pulldown's HTML serializer leaves double quotes unescaped in text/code contexts where cmark-gfm emits `&quot;`; the Markdown event signatures are stable, so this is render-spelling drift rather than formatter rewrite risk. |
| * | * | pulldown-html-mismatch:table-rendering | pulldown-html-mismatch | document | pulldown's table renderer minifies table markup, emits CSS `text-align` styles for alignment, and includes an empty `<tbody>` where cmark-gfm omits it. |
| * | * | pulldown-html-mismatch:tasklist-rendering | pulldown-html-mismatch | document | task-list checkbox HTML spelling is implementation-defined in the upstream spec; pulldown's renderer places checkbox inputs on their own line and uses different attribute ordering/empty-element spelling. |
| * | * | extension-gap:myst-definition-list | extension-gap | document | cmark-gfm does not own MyST directive syntax; mdwright's default definition-list recognition can make directive-heavy fixtures render differently through pulldown HTML, while formatter preservation is handled by mdwright document facts. |
| corpus | external:jupyter_book_minimal/admonitions.md, external:jupyter_book_minimal/asides.md, external:jupyter_book_minimal/blocks.md, external:jupyter_book_minimal/directives.md | sourcepos-risk:paragraph | extension-gap | document | cmark-gfm reports MyST directive/admonition syntax as ordinary paragraph source ranges, while mdwright treats the same bytes as extension-owned containers or preservation facts. The corpus rows pin that non-GFM coordinate drift so it cannot silently expand. |
| gfm-spec | case-144 | pulldown-html-mismatch:html-block-rendering | pulldown-html-mismatch | document | pulldown and cmark-gfm render equivalent HTML-block/list structure with different newline placement around the raw `<div>`. |
| gfm-spec | case-398, case-426, case-434, case-435, case-436, case-473, case-474, case-475, case-477 | pulldown-html-mismatch:emphasis-resolution | pulldown-html-mismatch | document | pulldown's emphasis resolution differs from cmark-gfm on these delimiter-stack edge cases; mdwright currently treats this as a parser-backend conformance gap, not a formatter-local bug. |
| operational | known-pulldown-link-ref-tab-panic | upstream-panic | upstream-panic | document | pulldown-cmark issue 1095 is contained by `mdwright-document::ParseError`; product paths do not panic. |

## Replacement Criteria

Do not replace `pulldown-cmark` based on event-shape differences alone. A replacement candidate must improve at least
one release-relevant axis without regressing the others:

- fewer unclassified or policy-relevant HTML mismatches against cmark-gfm;
- safer behaviour on malformed/user input;
- stable byte/source coordinates sufficient for formatter rewrite ownership;
- extension coverage at least as good as the current document facts;
- acceptable runtime and dependency footprint.
