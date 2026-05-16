# Known fuzz-found issues (deferred)

Inputs in this directory crash one of the fuzz harnesses but have
**not** been fixed yet — their fix has tradeoffs (GFM-spec regressions,
behaviour changes) that need design discussion before they land. They
are kept here so the fuzzer's seed corpus carries the minimised
reproducer; do not add them to `tests/regressions/` (the regression
suite would fail).

When you fix one, move the file to `tests/regressions/fuzz_<hash>.in`
(or `fuzz_<hash>.idem.in` if the fix is idempotence-only — see
`tests/regressions.rs`) and delete the matching entry from this
README.

## idempotence-unclosed-fenced-code-block.in

Bytes: `` ```` `` + control bytes + `\r#\t,` + `` ` `` (14 bytes).

Source starts with `` ```` `` — a fenced code block opener with no
matching closer in the input. mdwright's first format does NOT emit
a closing fence; the second format DOES. The two outputs differ at
the trailing line:

```
once:  …# ,\`\n
twice: …# ,\`\n````\n
```

Bug lives in the fenced-code-block emitter
(`src/cm/block/code_block.rs`-ish), which decides whether to write
a closing fence based on source structure rather than always
emitting one. Like the strikethrough escape and code-span padding
bugs already fixed, the right shape is a typed-construct invariant:
"emitted bytes round-trip to one `Event::Start(CodeBlock(Fenced))`
followed by content followed by `Event::End`." A constructor that
always emits the closer + a debug-time self-check would make this
class unrepresentable.
