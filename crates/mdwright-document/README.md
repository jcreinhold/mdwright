# mdwright-document

Recognised Markdown document facts with stable source coordinates, for
[mdwright](https://github.com/jcreinhold/mdwright).

This crate owns Markdown parsing for the whole workspace. It invokes pulldown-cmark
behind a containment boundary, recovers from parser panics, and produces typed
`Document` facts — headings, list groups, link definitions, frontmatter, code/HTML
exclusions, inline atoms, wrappable paragraphs, table sites, and the structural
ranges the formatter rewrites against. Every downstream crate (`mdwright-format`,
`mdwright-lint`, `mdwright-config`) consumes these facts; none of them sees
pulldown events directly.

Bytes-in / facts-out is the entire contract. Formatting decisions, lint rules, and
configuration policy belong to the other crates.

## Status

Pre-1.0. Public items are whatever `lib.rs` re-exports; breaking changes ship
without deprecation warnings.

## Use it

```toml
[dependencies]
mdwright-document = "0.1"
```

```rust
use mdwright_document::Document;
```

## See also

- Project: <https://github.com/jcreinhold/mdwright>
- Manual: <https://jcreinhold.github.io/mdwright/>
- Architecture: `docs/architecture/parser-boundary.md`,
  `docs/architecture/crate-boundaries.md`

## License

Licensed under MIT or Apache-2.0, at your option.
