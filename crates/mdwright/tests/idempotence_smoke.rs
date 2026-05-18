//! Idempotence smoke test: format(format(x)) == format(x).
//!
//! A formatter that doesn't reach a fixed point in two passes is
//! buggy: every CI run that re-formats a file would churn its
//! contents. This file exercises the inline serializers added in
//! session 08b (links, images, autolinks, breaks, inline HTML,
//! footnote refs); block-level idempotence is covered by
//! `tests/golden_block.rs`.

use mdwright_document::Document;
use mdwright_format::FmtOptions;

const SAMPLES: &[&str] = &[
    // Inline link, reference link, image, autolink, soft break,
    // emphasis, code span, footnote ref.
    "See [the docs](https://example.com \"docs\") and ![logo](https://example.com/x.png).\n\n\
     Reference: [foo][bar] or shortcut [bar].\n\nAutolink: <https://example.com>.\n\n\
     [bar]: https://example.com\n",
    // Hard break inside a paragraph; mixed inline HTML.
    "first line\\\nsecond <span>tag</span> line.\n",
    // Setext heading with a hard break (emits <br/> per CM).
    "first part\\\nsecond part\n===\n",
    // Footnote.
    "A claim.[^n]\n\n[^n]: explanation.\n",
    // Multiple reference link definitions scattered in source; the
    // formatter sorts them to the end of the document. The second
    // pass must see them in sorted order and emit them identically.
    "See [c][cee], [a][aye], and [b][bee].\n\n\
     [cee]: https://example.com/c\n[aye]: https://example.com/a\n[bee]: https://example.com/b\n",
];

#[test]
fn inline_serializers_reach_fixed_point() {
    let opts = FmtOptions::default();
    for (i, src) in SAMPLES.iter().enumerate() {
        let once = mdwright_format::format_document(&Document::parse(src), &opts);
        let twice = mdwright_format::format_document(&Document::parse(&once), &opts);
        assert_eq!(
            once, twice,
            "sample #{i} did not reach a fixed point\n--- once ---\n{once}--- twice ---\n{twice}",
        );
    }
}
