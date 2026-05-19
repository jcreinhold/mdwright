# Parser Boundary

`mdwright-document` is the only production crate that invokes `pulldown-cmark`.

Markdown is semantically total after mdwright canonicalises source bytes, but the parser implementation can still panic
on malformed edge cases. The document crate contains that implementation risk:

- source bytes are canonicalised into one parser input;
- parser iteration is collected eagerly inside one private `catch_unwind` boundary;
- parser panics become `mdwright_document::ParseError`;
- document facts, signatures, HTML rendering, and block checkpoints are built only after successful collection.

Callers do not catch parser panics. They parse source with:

```rust
let doc = mdwright_document::Document::parse(source)?;
```

Operations over an existing `Document` stay pure over recognised facts. Formatting a parsed document remains infallible:

```rust
let formatted = mdwright_format::format_document(&doc, &opts);
```

Source-convenience APIs are fallible because they cross the parser boundary:

```rust
let formatted = mdwright_format::format_source(source, &opts)?;
let html = mdwright_document::render_html(source)?;
```

The transactional formatter uses the same policy for verification reparses. If a candidate output cannot be parsed, the
candidate is rejected and the unverified bytes are not committed. CLI and LSP delivery turn `ParseError` into controlled
file/editor diagnostics; they do not install parser-specific panic handlers.
